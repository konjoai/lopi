//! Konjo interactive REPL — the primary `lopi` experience.
//!
//! Launched when the user runs `lopi` with no subcommand. Presents a
//! Claude-Code-style prompt where goals are typed inline and agent output
//! streams in real time.
mod actions;
mod draw;
mod input;
pub mod slash;
mod state;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lopi_core::LopiConfig;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

use self::{
    actions::{dispatch_goal, handle_slash},
    draw::{draw_help_overlay, draw_repl},
    input::InputAction,
    slash::autocomplete,
};
use crate::run_command;
pub use state::{LineStyle, ReplEvent, ReplMode, ReplState};

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
        actions::run_bypass(goal, repo, cfg).await
    } else {
        run_command::run(goal, repo, false, false, false, false, cfg).await
    }
}

/// The main TUI event loop.
async fn repl_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    repo: PathBuf,
    model: String,
    cfg: Option<&LopiConfig>,
) -> Result<()> {
    let mut state = ReplState::new(&repo, &model, cfg);
    let mut last_draw = Instant::now();
    let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<ReplEvent>();

    loop {
        if last_draw.elapsed() >= Duration::from_millis(250) {
            terminal.draw(|f| {
                draw_repl(f, &mut state);
                if state.show_help {
                    draw_help_overlay(f);
                }
            })?;
            last_draw = Instant::now();
        }

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

        if !event::poll(Duration::from_millis(50))? {
            continue;
        }

        let ev = event::read()?;
        match ev {
            Event::Key(key) => {
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
                last_draw = Instant::now() - Duration::from_secs(1);
            }
            _ => {}
        }
    }
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
