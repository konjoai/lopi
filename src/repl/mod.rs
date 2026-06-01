//! Konjo interactive REPL — the primary `lopi` experience.
//!
//! Launched when the user runs `lopi` with no subcommand. Presents a
//! Claude-Code-style prompt where goals are typed inline and agent output
//! streams in real time.
mod draw;
mod input;
pub mod slash;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lopi_core::{AgentEvent, LopiConfig, RepoProfile, Task, TaskSource};
use lopi_memory::MemoryStore;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    collections::VecDeque,
    io,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

use self::{
    draw::{draw_help_overlay, draw_repl},
    input::{InputAction, InputWidget},
    slash::{autocomplete, parse_slash, SlashCmd},
};
use crate::{run_command, task_commands, util::db_path};

/// A line in the scrollable output history.
#[derive(Clone)]
pub struct OutputLine {
    pub text: String,
    pub style: LineStyle,
}

/// Visual style for an output line.
#[derive(Clone, Copy)]
pub enum LineStyle {
    Normal,
    Success,
    Error,
    Info,
    AgentLog,
    Splash,
    Hint,
}

/// Whether an agent task is currently in flight.
pub enum ReplMode {
    Idle,
    Running,
}

/// All mutable state for the REPL session.
pub struct ReplState {
    pub repo_name: String,
    pub model_short: String,
    pub input: InputWidget,
    pub output_lines: VecDeque<OutputLine>,
    pub scroll_offset: usize,
    pub autocomplete: Vec<&'static slash::SlashDef>,
    pub mode: ReplMode,
    pub bypass: bool,
    pub session_cost_usd: f32,
    pub show_help: bool,
}

impl ReplState {
    fn new(repo: PathBuf, model: String, cfg: Option<&LopiConfig>) -> Self {
        let repo_name = crate::repo_detect::repo_display_name(&repo);
        let model_short = model
            .split('-')
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("-");
        let bypass = cfg.is_some_and(|c| c.lopi.bypass_permissions);

        let mut state = Self {
            repo_name,
            model_short,
            input: InputWidget::new(),
            output_lines: VecDeque::new(),
            scroll_offset: 0,
            autocomplete: Vec::new(),
            mode: ReplMode::Idle,
            bypass,
            session_cost_usd: 0.0,
            show_help: false,
        };
        state.splash();
        state
    }

    fn push(&mut self, text: impl Into<String>, style: LineStyle) {
        self.output_lines.push_back(OutputLine {
            text: text.into(),
            style,
        });
        // Clamp to last 2000 lines.
        while self.output_lines.len() > 2000 {
            self.output_lines.pop_front();
        }
        // Stay pinned to bottom unless user has scrolled up.
        if self.scroll_offset == 0 {
            // already at bottom — nothing to adjust
        }
    }

    fn splash(&mut self) {
        let art = [
            r"  ██╗      ██████╗ ██████╗ ██╗",
            r"  ██║     ██╔═══██╗██╔══██╗██║",
            r"  ██║     ██║   ██║██████╔╝██║",
            r"  ██║     ██║   ██║██╔═══╝ ██║",
            r"  ███████╗╚██████╔╝██║     ██║",
            r"  ╚══════╝ ╚═════╝ ╚═╝     ╚═╝",
        ];
        for line in art {
            self.push(line, LineStyle::Splash);
        }
        self.push("", LineStyle::Normal);
        self.push(
            "  Konjo agent orchestrator — beautiful, excellent, provably correct.",
            LineStyle::Hint,
        );
        self.push(
            "  Type a goal and press Enter, or type /help for commands.",
            LineStyle::Hint,
        );
        self.push("", LineStyle::Normal);
    }
}

/// Events flowing from a background agent run back to the REPL loop.
enum ReplEvent {
    AgentLog { line: String, style: LineStyle },
    TaskDone { label: String, success: bool },
    CostAccrued(f32),
}

/// Launch the interactive REPL TUI.
pub async fn run_repl(repo: PathBuf, model: String, cfg: Option<LopiConfig>) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = repl_loop(&mut terminal, repo, model, cfg.as_ref()).await;
    restore_terminal(&mut terminal)?;
    result
}

/// Run a single goal inline from a non-TUI context (`lopi "goal text"`).
pub async fn run_inline(
    goal: String,
    repo: PathBuf,
    bypass: bool,
    cfg: Option<&LopiConfig>,
) -> Result<()> {
    if bypass {
        run_bypass(goal, repo, cfg).await
    } else {
        run_command::run(goal, repo, false, false, false, false, cfg).await
    }
}

/// Execute a goal with directory restrictions disabled.
async fn run_bypass(goal: String, repo: PathBuf, cfg: Option<&LopiConfig>) -> Result<()> {
    use lopi_agent::AgentRunner;
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
    let id_short = &task_id.0.to_string()[..8];
    store.save_task(&task, "queued").await.ok();
    println!("   task id: {id_short}…");
    println!();

    if crate::util::is_self_modify_attempt(&repo) {
        let allow_self_modify = cfg.is_some_and(|c| c.lopi.allow_self_modify);
        if !allow_self_modify {
            eprintln!("❌ self-modification blocked in bypass mode");
            return Err(anyhow::anyhow!("self-modification not allowed"));
        }
    }

    let mut runner = AgentRunner::standalone(task.clone(), repo).0;
    runner.store = Some(store.clone());
    let bus = runner.bus.clone();

    let mut rx = bus.subscribe();
    let print_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(AgentEvent::StatusChanged {
                    status, attempt, ..
                }) => {
                    println!("  [{attempt}] → {}", crate::util::status_label(&status));
                }
                Ok(AgentEvent::LogLine { line, .. }) => println!("       {line}"),
                Ok(AgentEvent::TaskCompleted { .. }) | Err(_) => break,
                _ => {}
            }
        }
    });

    let outcome = runner.run().await?;
    print_task.abort();
    store
        .mark_completed(&task_id, &crate::util::status_label(&outcome))
        .await
        .ok();
    store.mine_patterns(&task_id, &task.goal).await.ok();
    println!();
    println!("⚓ {}", crate::util::status_label(&outcome));
    Ok(())
}

/// The main TUI event loop.
async fn repl_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    repo: PathBuf,
    model: String,
    cfg: Option<&LopiConfig>,
) -> Result<()> {
    let mut state = ReplState::new(repo.clone(), model, cfg);
    let mut last_draw = Instant::now();
    // Channel for background agent events → main loop.
    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<ReplEvent>();

    loop {
        // Draw if dirty or after 250ms.
        if last_draw.elapsed() >= Duration::from_millis(250) {
            terminal.draw(|f| {
                draw_repl(f, &mut state);
                if state.show_help {
                    draw_help_overlay(f);
                }
            })?;
            last_draw = Instant::now();
        }

        // Drain background agent events.
        while let Ok(ev) = ev_rx.try_recv() {
            match ev {
                ReplEvent::AgentLog { line, style } => state.push(line, style),
                ReplEvent::TaskDone { label, success } => {
                    state.push(
                        label,
                        if success {
                            LineStyle::Success
                        } else {
                            LineStyle::Error
                        },
                    );
                    state.mode = ReplMode::Idle;
                }
                ReplEvent::CostAccrued(usd) => state.session_cost_usd += usd,
            }
        }

        // Poll for terminal events with a short timeout so we stay responsive.
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        let ev = event::read()?;
        match ev {
            Event::Key(key) => {
                // Global quit when idle.
                if matches!(state.mode, ReplMode::Idle) {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => return Ok(()),
                        (KeyCode::F(1), _) | (KeyCode::Char('?'), _) => {
                            state.show_help = !state.show_help;
                            continue;
                        }
                        (KeyCode::Esc, _) if state.show_help => {
                            state.show_help = false;
                            continue;
                        }
                        (KeyCode::PageUp, _) => {
                            state.scroll_offset = state.scroll_offset.saturating_add(10);
                            continue;
                        }
                        (KeyCode::PageDown, _) => {
                            state.scroll_offset = state.scroll_offset.saturating_sub(10);
                            continue;
                        }
                        _ => {}
                    }
                }

                // Update autocomplete as user types.
                let action = state.input.handle_key(key);
                let val = state.input.value();
                if val.starts_with('/') {
                    let bare = val.strip_prefix('/').unwrap_or("");
                    let name = bare
                        .split_once(char::is_whitespace)
                        .map(|(n, _)| n)
                        .unwrap_or(bare);
                    state.autocomplete = autocomplete(name);
                } else {
                    state.autocomplete.clear();
                }

                match action {
                    InputAction::None => {}
                    InputAction::Escape => {
                        if state.show_help {
                            state.show_help = false;
                        } else if matches!(state.mode, ReplMode::Idle) {
                            return Ok(());
                        }
                    }
                    InputAction::Submit(text) => {
                        state.scroll_offset = 0;
                        state.show_help = false;
                        if text.starts_with('/') {
                            handle_slash(&text, &mut state, &repo, cfg, &ev_tx).await?;
                        } else {
                            dispatch_goal(
                                text,
                                &mut state,
                                repo.clone(),
                                false,
                                cfg,
                                ev_tx.clone(),
                            )
                            .await?;
                        }
                    }
                }
            }
            Event::Resize(_, _) => {
                // Force a redraw on next iteration.
                last_draw = Instant::now() - Duration::from_secs(1);
            }
            _ => {}
        }
    }
}

/// Dispatch a slash command from within the REPL.
async fn handle_slash(
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
            // Signal quit by returning a sentinel — caller returns Ok(()).
            // We use an escape hatch: push a quit marker that the caller checks.
            // Simpler: we use a direct return from the outer function by
            // re-using InputAction::Escape — here we just set a quit flag.
            // Since we can't return to the outer loop directly, push a sentinel line
            // and check it. Easiest: just exit the process.
            restore_terminal_raw()?;
            std::process::exit(0);
        }
        Ok(SlashCmd::Watch) => {
            restore_terminal_raw()?;
            task_commands::watch(None, true).await?;
            // On watch exit, re-setup (not worth re-entering REPL — just exit cleanly).
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

/// Spawn an agent run and wire its events back to the REPL loop via `ev_tx`.
async fn dispatch_goal(
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

    use lopi_agent::AgentRunner;
    use lopi_core::AgentEvent;

    let store = MemoryStore::open(db_path()).await?;
    let profile = if !bypass {
        RepoProfile::load_from_repo(&repo)
    } else {
        RepoProfile::default()
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
                    use lopi_core::TaskStatus;
                    let label = crate::util::status_label(&status);
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
                    let label = crate::util::status_label(&outcome);
                    let success = matches!(outcome, lopi_core::TaskStatus::Success { .. });
                    let _ = tx.send(ReplEvent::TaskDone { label, success });
                    break;
                }
                Err(_) => break,
                _ => {}
            }
        }
    });

    // Run the agent on a separate task.
    let tx2 = ev_tx;
    tokio::spawn(async move {
        let outcome = runner.run().await;
        let _ = store
            .mark_completed(
                &task_id,
                &crate::util::status_label(&outcome.unwrap_or(lopi_core::TaskStatus::Failed {
                    reason: "runner error".into(),
                })),
            )
            .await;
        let _ = store.mine_patterns(&task_id, &task.goal).await;
        // Send a done sentinel in case the bridge task missed TaskCompleted.
        let _ = tx2.send(ReplEvent::TaskDone {
            label: "⚓ done".into(),
            success: false,
        });
    });

    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal<B: ratatui::backend::Backend + io::Write>(
    terminal: &mut Terminal<B>,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Restore the terminal without a `Terminal` handle (used in slash-command exits).
fn restore_terminal_raw() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}
