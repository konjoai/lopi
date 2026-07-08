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
    ev_tx: &mpsc::UnboundedSender<ReplEvent>,
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
            task_commands::watch(None, true).await?;
            std::process::exit(0);
        }
        Ok(SlashCmd::Dock) => {
            restore_terminal_raw()?;
            task_commands::dock().await?;
            std::process::exit(0);
        }
        Ok(SlashCmd::Cancel { id }) => {
            restore_terminal_raw()?;
            task_commands::cancel(id).await?;
            std::process::exit(0);
        }
        Ok(SlashCmd::Cost) => {
            state.push(
                format!("  session cost: ${:.4}", state.session_cost_usd),
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

/// Spawn an agent run and bridge its events back to the REPL loop via `ev_tx`.
pub(super) async fn dispatch_goal(
    goal: String,
    state: &mut ReplState,
    repo: PathBuf,
    bypass: bool,
    _cfg: Option<&LopiConfig>,
    ev_tx: mpsc::UnboundedSender<ReplEvent>,
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

    // Bridge AgentEvent → ReplEvent on a background task.
    let tx = ev_tx.clone();
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(AgentEvent::StatusChanged {
                    status, attempt, ..
                }) => {
                    let label = status_label(&status);
                    let style = match &status {
                        TaskStatus::Success { .. } => LineStyle::Success,
                        TaskStatus::Failed { .. } | TaskStatus::RolledBack => LineStyle::Error,
                        _ => LineStyle::AgentLog,
                    };
                    let _ = tx.send(ReplEvent::AgentLog {
                        line: format!("  [{attempt}] → {label}"),
                        style,
                    });
                }
                Ok(AgentEvent::LogLine { line, .. }) => {
                    let _ = tx.send(ReplEvent::AgentLog {
                        line: format!("       {line}"),
                        style: LineStyle::AgentLog,
                    });
                }
                Ok(AgentEvent::TurnMetrics { cost_usd, .. }) => {
                    let _ = tx.send(ReplEvent::CostAccrued(cost_usd));
                }
                Ok(AgentEvent::TaskCompleted { outcome, .. }) => {
                    let label = status_label(&outcome);
                    let success = matches!(outcome, TaskStatus::Success { .. });
                    let _ = tx.send(ReplEvent::TaskDone { label, success });
                    break;
                }
                Err(_) => break,
                _ => {}
            }
        }
    });

    // Run the agent on a background task; send a done sentinel when finished.
    let tx2 = ev_tx;
    tokio::spawn(async move {
        let outcome = runner.run().await;
        let _ = store
            .mark_completed(
                &task_id,
                &status_label(&outcome.unwrap_or(TaskStatus::Failed {
                    reason: "runner error".into(),
                })),
            )
            .await;
        let _ = store.mine_patterns(&task_id, &task.goal).await;
        let _ = tx2.send(ReplEvent::TaskDone {
            label: "⚓ done".into(),
            success: false,
        });
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
