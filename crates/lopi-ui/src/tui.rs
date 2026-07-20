use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lopi_core::{AgentEvent, EventBus, LogLevel, TaskId, TaskStatus};
use ratatui::{backend::CrosstermBackend, style::Color, widgets::TableState, Terminal};
use std::collections::{HashMap, VecDeque};
use std::io;
use std::time::{Duration, Instant};

mod draw;

const MAX_LOG_LINES: usize = 200;
const DISPLAY_LOGS: usize = 20;
pub(super) const KONJO_PURPLE: Color = Color::Rgb(124, 58, 237);
pub(super) const KONJO_DIM: Color = Color::Rgb(60, 60, 80);

#[derive(Debug, Clone)]
pub(super) struct AgentRow {
    pub(super) id: TaskId,
    pub(super) goal: String,
    pub(super) status: TaskStatus,
    pub(super) attempt: u8,
    pub(super) max_retries: u8,
    pub(super) score: f32,
    pub(super) branch: String,
    pub(super) started: Instant,
}

impl AgentRow {
    pub(super) fn status_label(&self) -> (&str, Color) {
        match &self.status {
            TaskStatus::Queued => ("⏳ Queued", Color::Yellow),
            TaskStatus::Planning => ("📋 Planning", Color::Cyan),
            TaskStatus::AwaitingPlanApproval { .. } => ("⏸ Awaiting approval", Color::Yellow),
            TaskStatus::Implementing => ("🔨 Implementing", Color::Blue),
            TaskStatus::Testing => ("🧪 Testing", Color::Magenta),
            TaskStatus::Scoring => ("📊 Scoring", Color::Cyan),
            TaskStatus::Retrying { .. } => ("♻️ Retrying", Color::LightYellow),
            TaskStatus::Success { .. } => ("✅ Done", Color::Green),
            TaskStatus::Failed { .. } => ("❌ Failed", Color::Red),
            TaskStatus::RolledBack => ("⏪ Rolled back", Color::Red),
            TaskStatus::Conflict { .. } => ("⚠ Conflict", Color::Red),
        }
    }

    pub(super) fn elapsed(&self) -> String {
        let s = self.started.elapsed().as_secs();
        if s < 60 {
            format!("{s}s")
        } else if s < 3600 {
            format!("{}m{}s", s / 60, s % 60)
        } else {
            format!("{}h{}m", s / 3600, (s % 3600) / 60)
        }
    }
}

pub(super) struct AppState {
    pub(super) agents: HashMap<TaskId, AgentRow>,
    log_lines: VecDeque<LogEntry>,
    pub(super) table_state: TableState,
    pub(super) selected_task: Option<TaskId>,
    pub(super) log_filter: Option<TaskId>,
    pub(super) show_help: bool,
    pub(super) queued_count: usize,
    pub(super) succeeded: usize,
    pub(super) failed: usize,
    started_at: Instant,
}

#[derive(Debug)]
pub(super) struct LogEntry {
    pub(super) task_id: TaskId,
    pub(super) line: String,
    pub(super) level: LogLevel,
}

impl AppState {
    fn new() -> Self {
        Self {
            agents: HashMap::new(),
            log_lines: VecDeque::new(),
            table_state: TableState::default(),
            selected_task: None,
            log_filter: None,
            show_help: false,
            queued_count: 0,
            succeeded: 0,
            failed: 0,
            started_at: Instant::now(),
        }
    }

    fn handle_event(&mut self, ev: AgentEvent) {
        match ev {
            AgentEvent::TaskQueued { task_id, goal, .. } => {
                self.queued_count += 1;
                let row = AgentRow {
                    id: task_id,
                    goal,
                    status: TaskStatus::Queued,
                    attempt: 0,
                    max_retries: 3,
                    score: 0.0,
                    branch: String::new(),
                    started: Instant::now(),
                };
                self.agents.insert(task_id, row);
            }
            AgentEvent::TaskStarted {
                task_id,
                attempt,
                branch,
            } => {
                if let Some(a) = self.agents.get_mut(&task_id) {
                    a.attempt = attempt;
                    a.branch = branch;
                    a.started = Instant::now();
                }
            }
            AgentEvent::StatusChanged {
                task_id,
                status,
                attempt,
            } => {
                if let Some(a) = self.agents.get_mut(&task_id) {
                    a.status = status;
                    a.attempt = attempt;
                }
            }
            AgentEvent::ScoreUpdated {
                task_id,
                test_pass_rate,
                ..
            } => {
                if let Some(a) = self.agents.get_mut(&task_id) {
                    a.score = test_pass_rate;
                }
            }
            AgentEvent::LogLine {
                task_id,
                line,
                level,
                ..
            } => {
                self.log_lines.push_back(LogEntry {
                    task_id,
                    line,
                    level,
                });
                if self.log_lines.len() > MAX_LOG_LINES {
                    self.log_lines.pop_front();
                }
            }
            AgentEvent::TaskCompleted {
                task_id, outcome, ..
            } => {
                if let Some(a) = self.agents.get_mut(&task_id) {
                    a.status = outcome.clone();
                }
                match outcome {
                    TaskStatus::Success { .. } => self.succeeded += 1,
                    TaskStatus::Failed { .. } => self.failed += 1,
                    _ => {}
                }
            }
            AgentEvent::TaskCancelled { task_id } => {
                self.agents.remove(&task_id);
            }
            AgentEvent::PoolStats { queued, .. } => {
                self.queued_count = queued;
            }
            // TurnMetrics drives the web UI's Forge shader. The TUI doesn't
            // visualize per-turn pressure/activity — silently consume.
            AgentEvent::TurnMetrics { .. } => {}
            // BudgetExceeded is shown in the web Forge with a flashing pill
            // and surfaced via /metrics — the TUI is read-only, no action needed.
            AgentEvent::BudgetExceeded { .. } => {}
            // BudgetSoftWarn (Part 4.2) is surfaced via a tracing::warn! log
            // line and Telegram — the read-only TUI has no dedicated panel.
            AgentEvent::BudgetSoftWarn { .. } => {}
            // VerifierVerdict is surfaced via the task log stream; TUI doesn't
            // render a separate panel for it.
            AgentEvent::VerifierVerdict { .. } => {}
            // PlanProposed is reflected by the AwaitingPlanApproval status the
            // runner emits alongside it; the read-only TUI shows that label.
            AgentEvent::PlanProposed { .. } => {}
            // The stream-json pane events (tool calls, token/cost/phase/rate
            // limit) drive the web Forge's gauges. The read-only TUI surfaces
            // the same activity through the LogLine stream, so consume silently.
            AgentEvent::ToolCall { .. }
            | AgentEvent::ToolResult { .. }
            | AgentEvent::TokenDelta { .. }
            | AgentEvent::ApiRetry { .. }
            | AgentEvent::Cost { .. }
            | AgentEvent::Phase { .. } => {}
            // Report on Finish is delivered by lopi-remote's Telegram
            // notifier; the read-only TUI has no channel to route it to.
            AgentEvent::ReportReady { .. } => {}
        }
    }

    pub(super) fn sorted_agents(&self) -> Vec<&AgentRow> {
        let mut v: Vec<&AgentRow> = self.agents.values().collect();
        v.sort_by_key(|a| a.started);
        v
    }

    fn selected_id(&self) -> Option<TaskId> {
        let agents = self.sorted_agents();
        self.table_state
            .selected()
            .and_then(|i| agents.get(i))
            .map(|a| a.id)
    }

    pub(super) fn uptime(&self) -> String {
        let s = self.started_at.elapsed().as_secs();
        if s < 60 {
            format!("{s}s")
        } else if s < 3600 {
            format!("{}m{}s", s / 60, s % 60)
        } else {
            format!("{}h{}m", s / 3600, (s % 3600) / 60)
        }
    }

    pub(super) fn visible_logs(&self) -> Vec<&LogEntry> {
        self.log_lines
            .iter()
            .filter(|e| self.log_filter.is_none_or(|id| e.task_id == id))
            .rev()
            .take(DISPLAY_LOGS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

/// # Errors
///
/// Returns an error if the terminal cannot be initialized or the event loop fails.
pub async fn run(bus: EventBus<AgentEvent>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    // `run_loop` blocks on crossterm's synchronous terminal I/O
    // (`event::poll`/`event::read`) for the whole TUI session — running it
    // inline on this async task would pin its tokio worker thread for that
    // entire duration instead of yielding, starving any other task
    // scheduled on it (e.g. `src/remote.rs` runs a WebSocket-pump task
    // concurrently with this on the same bus). Run it on the blocking pool.
    let (mut terminal, res) = tokio::task::spawn_blocking(move || {
        let mut terminal = terminal;
        let res = run_loop(&mut terminal, &bus);
        (terminal, res)
    })
    .await
    .context("TUI event loop task panicked")?;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    res
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    bus: &EventBus<AgentEvent>,
) -> Result<()> {
    // Elapsed timers in AgentRow update every second — refresh at that cadence when idle.
    const TIMER_INTERVAL: Duration = Duration::from_secs(1);
    let mut state = AppState::new();
    let mut rx = bus.subscribe();
    let mut needs_redraw = true;
    let mut last_timer_redraw = Instant::now();

    loop {
        // Drain all pending agent events; mark dirty on any arrival.
        loop {
            match rx.try_recv() {
                Ok(ev) => {
                    state.handle_event(ev);
                    needs_redraw = true;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!("TUI lagged {n} events");
                    needs_redraw = true;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => return Ok(()),
            }
        }

        // Periodic timer refresh so elapsed counters don't freeze when idle.
        if last_timer_redraw.elapsed() >= TIMER_INTERVAL {
            needs_redraw = true;
            last_timer_redraw = Instant::now();
        }

        if needs_redraw {
            terminal.draw(|f| draw::draw(f, &mut state))?;
            needs_redraw = false;
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    (KeyCode::Char('?') | KeyCode::F(1), _) => {
                        state.show_help = !state.show_help;
                    }
                    (KeyCode::Down | KeyCode::Char('j'), _) => {
                        let len = state.agents.len();
                        if len > 0 {
                            let i = state.table_state.selected().map_or(0, |i| (i + 1) % len);
                            state.table_state.select(Some(i));
                        }
                    }
                    (KeyCode::Up | KeyCode::Char('k'), _) => {
                        let len = state.agents.len();
                        if len > 0 {
                            let i = state.table_state.selected().map_or(0, |i| {
                                if i == 0 {
                                    len - 1
                                } else {
                                    i - 1
                                }
                            });
                            state.table_state.select(Some(i));
                        }
                    }
                    (KeyCode::Enter, _) => {
                        state.selected_task = state.selected_id();
                        state.log_filter = state.selected_task;
                    }
                    (KeyCode::Esc, _) => {
                        state.log_filter = None;
                        state.selected_task = None;
                    }
                    (KeyCode::Char('l'), _) => {
                        // Toggle log filter for selected task.
                        if state.log_filter.is_some() {
                            state.log_filter = None;
                        } else {
                            state.log_filter = state.selected_id();
                        }
                    }
                    _ => {}
                }
                needs_redraw = true;
            }
        }
    }
}
