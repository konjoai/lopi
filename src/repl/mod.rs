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
    let mut events = CrosstermEvents;
    let result = repl_loop(&mut terminal, &mut events, repo, model, cfg.as_ref()).await;
    restore_terminal(&mut terminal)?;
    result
}

/// Abstraction over the terminal-input source, injected so `repl_loop`'s dispatch
/// logic is testable without a real TTY (`CrosstermEvents` in production, a scripted
/// fake in tests below).
trait EventSource {
    /// Poll-then-read one terminal event, or `Ok(None)` if `timeout` elapsed with
    /// nothing to read (mirrors `crossterm::event::poll` + `event::read`).
    fn next_event(&mut self, timeout: Duration) -> io::Result<Option<Event>>;
}

struct CrosstermEvents;

impl EventSource for CrosstermEvents {
    fn next_event(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        if event::poll(timeout)? {
            Ok(Some(event::read()?))
        } else {
            Ok(None)
        }
    }
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
        run_command::run(
            goal,
            repo,
            false,
            false,
            false,
            false,
            cfg,
            run_command::BudgetArgs::default(),
        )
        .await
    }
}

/// The main TUI event loop.
async fn repl_loop<B: ratatui::backend::Backend, E: EventSource>(
    terminal: &mut Terminal<B>,
    events: &mut E,
    repo: PathBuf,
    model: String,
    cfg: Option<&LopiConfig>,
) -> Result<()> {
    let mut state = ReplState::new(&repo, &model, cfg);
    // Bounded (Sprint S13R, Phase D — the panic/resource surface pass): an unbounded
    // channel here meant an agent run logging faster than the REPL redraws could grow
    // this queue without limit. 1024 is generous for interactive log-line volume — a
    // background sender backpressures via `.await` on `send`, never drops silently.
    let (ev_tx, mut ev_rx) = mpsc::channel::<ReplEvent>(1024);

    // First paint — without this the screen stays blank until the first event.
    terminal.draw(|f| {
        draw_repl(f, &mut state);
    })?;
    let mut last_draw = Instant::now();

    loop {
        // Drain background agent events; note whether anything changed.
        let mut agent_updated = false;
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
            agent_updated = true;
        }

        // Throttle agent-log redraws to 50 ms; key events trigger an
        // immediate redraw further below so typing feels instant.
        if agent_updated && last_draw.elapsed() >= Duration::from_millis(50) {
            state.anim_tick = state.anim_tick.wrapping_add(1);
            terminal.draw(|f| {
                draw_repl(f, &mut state);
                if state.show_help {
                    draw_help_overlay(f);
                }
            })?;
            last_draw = Instant::now();
        }

        // Drive the spinner animation when the agent is running even if no
        // log events are arriving (e.g. long silent LLM call).
        if matches!(state.mode, ReplMode::Running)
            && last_draw.elapsed() >= Duration::from_millis(120)
        {
            state.anim_tick = state.anim_tick.wrapping_add(1);
            terminal.draw(|f| {
                draw_repl(f, &mut state);
            })?;
            last_draw = Instant::now();
        }

        let Some(ev) = events.next_event(Duration::from_millis(16))? else {
            continue;
        };
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
                // Redraw immediately so keystrokes appear without any delay.
                terminal.draw(|f| {
                    draw_repl(f, &mut state);
                    if state.show_help {
                        draw_help_overlay(f);
                    }
                })?;
                last_draw = Instant::now();
            }
            Event::Resize(_, _) => {
                // Redraw immediately on resize.
                terminal.draw(|f| {
                    draw_repl(f, &mut state);
                    if state.show_help {
                        draw_help_overlay(f);
                    }
                })?;
                last_draw = Instant::now();
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::backend::TestBackend;
    use std::collections::VecDeque;

    /// A scripted `EventSource`: yields the queued events in order, then `None`
    /// forever (as if the poll interval kept elapsing with nothing typed).
    struct ScriptedEvents(VecDeque<Event>);

    impl EventSource for ScriptedEvents {
        fn next_event(&mut self, _timeout: Duration) -> io::Result<Option<Event>> {
            Ok(self.0.pop_front())
        }
    }

    fn esc_key() -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    /// Mutation-testing kill test: a mutant replacing `repl_loop`'s body with
    /// `Ok(())` would also return `Ok(())` here trivially — the real signal this
    /// test provides is that the loop actually consumes the scripted `Esc` event
    /// and returns *because* of it. A mutant deleting the `InputAction::Escape`
    /// handling would instead loop forever once the scripted queue runs dry
    /// (`ScriptedEvents` returns `None` indefinitely after) — bounded with a
    /// timeout so that failure mode is a clean test failure, not a hung CI job.
    #[tokio::test]
    async fn repl_loop_exits_cleanly_on_escape_while_idle() {
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut events = ScriptedEvents(VecDeque::from([esc_key()]));

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            repl_loop(
                &mut terminal,
                &mut events,
                PathBuf::from("."),
                "claude-sonnet-5".to_string(),
                None,
            ),
        )
        .await
        .expect("repl_loop did not return within 5s — likely stuck in an event-poll loop");

        assert!(result.is_ok());
        // A whole-body "replace repl_loop with Ok(())" mutant would also satisfy the
        // assertion above without ever touching `events` — this is the assertion that
        // actually distinguishes it: only real processing drains the scripted queue.
        assert!(
            events.0.is_empty(),
            "the scripted Esc event was never consumed"
        );
    }

    /// Mutation-testing kill test for `run_repl` (a "replace with Ok(())" mutant):
    /// only runs when stdout has no real controlling terminal (true for every
    /// automated runner — CI, `cargo mutants`, this workspace's own pre-commit
    /// hook — confirmed empirically: `enable_raw_mode()` returns `ENXIO` there),
    /// in which case `setup_terminal()`'s `enable_raw_mode()?` genuinely fails
    /// fast, which the mutant would not. Skips itself rather than actually
    /// entering raw mode when a real terminal IS attached (a developer running
    /// `cargo test` from an interactive shell), so it can never hang or clobber
    /// someone's live terminal session.
    #[tokio::test]
    async fn run_repl_fails_fast_with_no_controlling_terminal() {
        use std::io::IsTerminal;
        if std::io::stdout().is_terminal() {
            return;
        }
        let result = run_repl(PathBuf::from("."), "claude-sonnet-5".to_string(), None).await;
        assert!(result.is_err());
    }
}
