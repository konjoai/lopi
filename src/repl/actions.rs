//! Slash-command dispatch and goal-execution helpers for the Konjo REPL.
use anyhow::Result;
use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use lopi_agent::AgentRunner;
use lopi_core::{AgentEvent, LopiConfig, RepoProfile, Task, TaskSource, TaskStatus};
use lopi_memory::MemoryStore;
use std::{io, path::PathBuf};
use tokio::sync::mpsc;

use super::{
    slash::{parse_slash, SlashCmd},
    state::{LineStyle, ReplEvent, ReplMode, ReplState},
};
use crate::{
    task_commands,
    util::{db_path, is_self_modify_attempt, status_label},
};

/// Dispatch a slash command entered in the REPL prompt.
pub(super) async fn handle_slash(
    text: &str,
    state: &mut ReplState,
    repo: &std::path::Path,
    cfg: Option<&LopiConfig>,
    ev_tx: &mpsc::Sender<ReplEvent>,
) -> Result<()> {
    match parse_slash(text) {
        Err(msg) => state.push(msg, LineStyle::Error),
        Ok(SlashCmd::Help) => state.show_help = true,
        Ok(SlashCmd::Clear) => {
            state.output_lines.clear();
            state.scroll_offset = 0;
        }
        Ok(SlashCmd::Quit) => {
            restore_terminal_raw()?;
            std::process::exit(0);
        }
        Ok(SlashCmd::Watch) => {
            restore_terminal_raw()?;
            task_commands::watch(None, true, false).await?;
            std::process::exit(0);
        }
        Ok(SlashCmd::Dock) => {
            restore_terminal_raw()?;
            task_commands::dock().await?;
            std::process::exit(0);
        }
        Ok(SlashCmd::Cancel { id }) => {
            restore_terminal_raw()?;
            println!(
                "{}",
                task_commands::cancel("http://127.0.0.1:3000", id).await?
            );
            std::process::exit(0);
        }
        Ok(SlashCmd::Cost) => {
            state.push(
                format!(
                    "  session cost: ${:.4} — local token burn, counted by lopi from this \
                     session's own runs. Not your plan quota, not account usage, not a bill, \
                     and not inclusive of any Claude Code session run by hand outside lopi.",
                    state.session_cost_usd
                ),
                LineStyle::Info,
            );
        }
        Ok(SlashCmd::Model { name }) => match name {
            None => state.push(format!("  model: {}", state.model_short), LineStyle::Info),
            Some(m) => {
                state.model_short = m.clone();
                state.push(format!("  model set to: {m}"), LineStyle::Info);
            }
        },
        Ok(SlashCmd::Run { goal }) => {
            dispatch_goal(goal, state, repo.to_path_buf(), false, cfg, ev_tx.clone()).await?;
        }
        Ok(SlashCmd::Bypass { goal }) => {
            dispatch_goal(goal, state, repo.to_path_buf(), true, cfg, ev_tx.clone()).await?;
        }
    }
    Ok(())
}

/// Translate one `AgentEvent` (or a broadcast-channel error) into the `ReplEvent` the
/// bridge loop should forward, if any, plus whether the loop should stop after this.
/// Pure and synchronous so the four event shapes below are unit-tested directly rather
/// than only reachable through a full `AgentRunner` run.
fn translate_agent_event(
    ev: std::result::Result<AgentEvent, tokio::sync::broadcast::error::RecvError>,
) -> (Option<ReplEvent>, bool) {
    match ev {
        Ok(AgentEvent::StatusChanged {
            status, attempt, ..
        }) => {
            let label = status_label(&status);
            let style = match &status {
                TaskStatus::Success { .. } => LineStyle::Success,
                TaskStatus::Failed { .. } | TaskStatus::RolledBack => LineStyle::Error,
                _ => LineStyle::AgentLog,
            };
            (
                Some(ReplEvent::AgentLog {
                    line: format!("  [{attempt}] → {label}"),
                    style,
                }),
                false,
            )
        }
        Ok(AgentEvent::LogLine { line, .. }) => (
            Some(ReplEvent::AgentLog {
                line: format!("       {line}"),
                style: LineStyle::AgentLog,
            }),
            false,
        ),
        Ok(AgentEvent::TurnMetrics { cost_usd, .. }) => {
            (Some(ReplEvent::CostAccrued(cost_usd)), false)
        }
        Ok(AgentEvent::TaskCompleted { outcome, .. }) => {
            let label = status_label(&outcome);
            let success = matches!(outcome, TaskStatus::Success { .. });
            (Some(ReplEvent::TaskDone { label, success }), true)
        }
        Err(_) => (None, true),
        _ => (None, false),
    }
}

/// Spawn an agent run and bridge its events back to the REPL loop via `ev_tx`.
pub(super) async fn dispatch_goal(
    goal: String,
    state: &mut ReplState,
    repo: PathBuf,
    bypass: bool,
    _cfg: Option<&LopiConfig>,
    ev_tx: mpsc::Sender<ReplEvent>,
) -> Result<()> {
    if matches!(state.mode, ReplMode::Running) {
        state.push(
            "⚠ agent already running — wait for it to finish",
            LineStyle::Error,
        );
        return Ok(());
    }

    let store = MemoryStore::open(db_path()).await?;
    let profile = if bypass {
        RepoProfile::default()
    } else {
        RepoProfile::load_from_repo(&repo)
    };

    let mut task = Task::new(goal.clone());
    if bypass {
        task.allowed_dirs = Vec::new();
        task.forbidden_dirs = Vec::new();
    } else {
        profile.apply(&mut task);
    }

    let task_id = task.id;
    store.save_task(&task, "queued").await.ok();

    state.push(format!("▶ {goal}"), LineStyle::Info);
    state.mode = ReplMode::Running;

    let mut runner = AgentRunner::standalone(task.clone(), repo).0;
    runner.store = Some(store.clone());
    let bus = runner.bus.clone();

    // Bridge AgentEvent → ReplEvent on a background task. The translation itself is
    // `translate_agent_event`, a pure function unit-tested below — this loop is just
    // the async plumbing around it (subscribe, send, stop on completion/lag).
    let tx = ev_tx.clone();
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        loop {
            let (out, stop) = translate_agent_event(rx.recv().await);
            if let Some(ev) = out {
                let _ = tx.send(ev).await;
            }
            if stop {
                break;
            }
        }
    });

    // Run the agent on a background task; send a done sentinel when finished.
    let tx2 = ev_tx;
    tokio::spawn(async move {
        let outcome = runner.run().await;
        // Persist the canonical status token, not the emoji display label.
        let final_status = outcome.unwrap_or(TaskStatus::Failed {
            reason: "runner error".into(),
        });
        let _ = store
            .mark_completed(&task_id, final_status.db_status())
            .await;
        // Constraint-Capture-2 — only a clean success has a constraint worth
        // seeding forward; see the matching comment in
        // `lopi_orchestrator::pool::run_loop`.
        let success_constraint = matches!(final_status, TaskStatus::Success { .. })
            .then(|| runner.success_constraint())
            .flatten();
        let _ = store
            .mine_patterns(&task_id, &task.goal, success_constraint.as_deref())
            .await;
        let _ = tx2
            .send(ReplEvent::TaskDone {
                label: "⚓ done".into(),
                success: false,
            })
            .await;
    });

    Ok(())
}

/// Execute a goal with directory restrictions disabled (non-TUI path).
pub(super) async fn run_bypass(
    goal: String,
    repo: PathBuf,
    cfg: Option<&LopiConfig>,
) -> Result<()> {
    println!("🚢 lopi run (bypass)");
    println!("   goal: {goal}");
    println!("   repo: {}", repo.display());
    println!("   ⚠️  bypass mode: directory restrictions disabled");
    println!();

    let store = MemoryStore::open(db_path()).await?;
    let mut task = Task::new(goal.clone());
    task.allowed_dirs = Vec::new();
    task.forbidden_dirs = Vec::new();
    task.source = TaskSource::Cli;

    let task_id = task.id;
    let id_str = task_id.0.to_string();
    let id_short = &id_str[..8.min(id_str.len())];
    store.save_task(&task, "queued").await.ok();
    println!("   task id: {id_short}…");
    println!();

    if is_self_modify_attempt(&repo) {
        let allow_self_modify = cfg.is_some_and(|c| c.lopi.allow_self_modify);
        if !allow_self_modify {
            eprintln!("❌ self-modification blocked in bypass mode");
            return Err(anyhow::anyhow!("self-modification not allowed"));
        }
    }

    let mut runner = AgentRunner::standalone(task.clone(), repo).0;
    runner.store = Some(store.clone());

    crate::run_command::run_with_live_print(runner, &store, task_id, &task.goal, false).await?;
    Ok(())
}

/// Restore the terminal without a `Terminal` handle (used before process exit in slash commands).
pub(super) fn restore_terminal_raw() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::repl::state::LineStyle;

    fn test_state() -> ReplState {
        ReplState::new(&std::path::PathBuf::from("."), "claude-sonnet-5", None)
    }

    /// Mutation-testing kill test: `handle_slash` has several branches
    /// (`Quit`/`Watch`/`Dock`/`Cancel`) that call `std::process::exit`,
    /// which a test process must never invoke — so this exercises only the
    /// non-exiting branches, but that's enough: a mutant replacing the
    /// whole function body with `Ok(())` would leave `state.show_help`
    /// unset, failing this assertion.
    #[tokio::test]
    async fn help_command_sets_show_help() {
        let mut state = test_state();
        let (tx, _rx) = mpsc::channel(16);
        assert!(!state.show_help);
        handle_slash("/help", &mut state, std::path::Path::new("."), None, &tx)
            .await
            .unwrap();
        assert!(state.show_help);
    }

    #[tokio::test]
    async fn clear_command_empties_output_and_resets_scroll() {
        let mut state = test_state();
        let (tx, _rx) = mpsc::channel(16);
        state.push("some prior output", LineStyle::Normal);
        state.scroll_offset = 5;
        handle_slash("/clear", &mut state, std::path::Path::new("."), None, &tx)
            .await
            .unwrap();
        assert!(state.output_lines.is_empty());
        assert_eq!(state.scroll_offset, 0);
    }

    #[tokio::test]
    async fn unrecognized_command_pushes_an_error_line() {
        let mut state = test_state();
        let (tx, _rx) = mpsc::channel(16);
        let before = state.output_lines.len();
        handle_slash(
            "/not-a-real-command",
            &mut state,
            std::path::Path::new("."),
            None,
            &tx,
        )
        .await
        .unwrap();
        assert!(state.output_lines.len() > before);
    }

    fn recv_err() -> std::result::Result<AgentEvent, tokio::sync::broadcast::error::RecvError> {
        Err(tokio::sync::broadcast::error::RecvError::Closed)
    }

    #[test]
    fn translate_status_changed_forwards_agent_log_and_continues() {
        let (out, stop) = translate_agent_event(Ok(AgentEvent::StatusChanged {
            task_id: lopi_core::TaskId::new(),
            status: TaskStatus::Implementing,
            attempt: 2,
        }));
        assert!(!stop);
        assert!(matches!(&out, Some(ReplEvent::AgentLog { .. })));
        if let Some(ReplEvent::AgentLog { line, style }) = out {
            assert!(line.contains("[2]"), "line: {line}");
            assert!(matches!(style, LineStyle::AgentLog));
        }
    }

    #[test]
    fn translate_log_line_forwards_agent_log_and_continues() {
        let (out, stop) = translate_agent_event(Ok(AgentEvent::LogLine {
            task_id: lopi_core::TaskId::new(),
            line: "building...".into(),
            level: lopi_core::LogLevel::Info,
            ts: chrono::Utc::now(),
        }));
        assert!(!stop);
        assert!(matches!(&out, Some(ReplEvent::AgentLog { .. })));
        if let Some(ReplEvent::AgentLog { line, .. }) = out {
            assert!(line.contains("building..."), "line: {line}");
        }
    }

    #[test]
    fn translate_turn_metrics_forwards_cost_accrued_and_continues() {
        let (out, stop) = translate_agent_event(Ok(AgentEvent::TurnMetrics {
            task_id: lopi_core::TaskId::new(),
            pressure: 0.1,
            activity: 0.2,
            tokens_per_sec: 10.0,
            cost_usd: 0.5,
        }));
        assert!(!stop);
        assert!(matches!(&out, Some(ReplEvent::CostAccrued(_))));
        if let Some(ReplEvent::CostAccrued(cost)) = out {
            assert!((cost - 0.5).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn translate_task_completed_forwards_task_done_and_stops() {
        let (out, stop) = translate_agent_event(Ok(AgentEvent::TaskCompleted {
            task_id: lopi_core::TaskId::new(),
            outcome: TaskStatus::Success {
                branch: "lopi/attempt-1".into(),
                pr_url: None,
            },
            total_attempts: 1,
            successor: None,
        }));
        assert!(stop, "TaskCompleted must stop the bridge loop");
        assert!(matches!(&out, Some(ReplEvent::TaskDone { .. })));
        if let Some(ReplEvent::TaskDone { success, .. }) = out {
            assert!(success);
        }
    }

    #[test]
    fn translate_recv_error_stops_with_no_event() {
        let (out, stop) = translate_agent_event(recv_err());
        assert!(stop, "a closed/lagged bus must stop the bridge loop");
        assert!(out.is_none());
    }
}
