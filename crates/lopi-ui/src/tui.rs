use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lopi_core::{AgentEvent, EventBus, LogLevel, TaskId, TaskStatus};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph,
        Row, Table, TableState, Wrap,
    },
    Frame, Terminal,
};
use std::collections::{HashMap, VecDeque};
use std::io;
use std::time::{Duration, Instant};

const MAX_LOG_LINES: usize = 200;
const DISPLAY_LOGS: usize = 20;
const KONJO_PURPLE: Color = Color::Rgb(124, 58, 237);
const KONJO_DIM: Color = Color::Rgb(60, 60, 80);

#[derive(Debug, Clone)]
struct AgentRow {
    id: TaskId,
    goal: String,
    status: TaskStatus,
    attempt: u8,
    max_retries: u8,
    score: f32,
    branch: String,
    started: Instant,
}

impl AgentRow {
    fn status_label(&self) -> (&str, Color) {
        match &self.status {
            TaskStatus::Queued => ("⏳ Queued", Color::Yellow),
            TaskStatus::Planning => ("📋 Planning", Color::Cyan),
            TaskStatus::Implementing => ("🔨 Implementing", Color::Blue),
            TaskStatus::Testing => ("🧪 Testing", Color::Magenta),
            TaskStatus::Scoring => ("📊 Scoring", Color::Cyan),
            TaskStatus::Retrying { .. } => ("♻️ Retrying", Color::LightYellow),
            TaskStatus::Success { .. } => ("✅ Done", Color::Green),
            TaskStatus::Failed { .. } => ("❌ Failed", Color::Red),
            TaskStatus::RolledBack => ("⏪ Rolled back", Color::Red),
        }
    }

    fn elapsed(&self) -> String {
        let s = self.started.elapsed().as_secs();
        if s < 60 { format!("{s}s") }
        else if s < 3600 { format!("{}m{}s", s / 60, s % 60) }
        else { format!("{}h{}m", s / 3600, (s % 3600) / 60) }
    }
}

struct AppState {
    agents: HashMap<TaskId, AgentRow>,
    log_lines: VecDeque<LogEntry>,
    table_state: TableState,
    selected_task: Option<TaskId>,
    log_filter: Option<TaskId>,
    show_help: bool,
    queued_count: usize,
    succeeded: usize,
    failed: usize,
    started_at: Instant,
}

#[derive(Debug)]
struct LogEntry {
    task_id: TaskId,
    line: String,
    level: LogLevel,
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
            AgentEvent::TaskStarted { task_id, attempt, branch } => {
                if let Some(a) = self.agents.get_mut(&task_id) {
                    a.attempt = attempt;
                    a.branch = branch;
                    a.started = Instant::now();
                }
            }
            AgentEvent::StatusChanged { task_id, status, attempt } => {
                if let Some(a) = self.agents.get_mut(&task_id) {
                    a.status = status;
                    a.attempt = attempt;
                }
            }
            AgentEvent::ScoreUpdated { task_id, test_pass_rate, .. } => {
                if let Some(a) = self.agents.get_mut(&task_id) {
                    a.score = test_pass_rate;
                }
            }
            AgentEvent::LogLine { task_id, line, level, .. } => {
                self.log_lines.push_back(LogEntry { task_id, line, level });
                if self.log_lines.len() > MAX_LOG_LINES {
                    self.log_lines.pop_front();
                }
            }
            AgentEvent::TaskCompleted { task_id, outcome, .. } => {
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
        }
    }

    fn sorted_agents(&self) -> Vec<&AgentRow> {
        let mut v: Vec<&AgentRow> = self.agents.values().collect();
        v.sort_by_key(|a| a.started);
        v
    }

    fn selected_id(&self) -> Option<TaskId> {
        let agents = self.sorted_agents();
        self.table_state.selected().and_then(|i| agents.get(i)).map(|a| a.id)
    }

    fn uptime(&self) -> String {
        let s = self.started_at.elapsed().as_secs();
        if s < 60 { format!("{s}s") }
        else if s < 3600 { format!("{}m{}s", s / 60, s % 60) }
        else { format!("{}h{}m", s / 3600, (s % 3600) / 60) }
    }

    fn visible_logs(&self) -> Vec<&LogEntry> {
        self.log_lines.iter()
            .filter(|e| self.log_filter.is_none_or(|id| e.task_id == id))
            .rev()
            .take(DISPLAY_LOGS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

pub async fn run(bus: EventBus<AgentEvent>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, bus).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    res
}

async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    bus: EventBus<AgentEvent>,
) -> Result<()> {
    let mut state = AppState::new();
    let mut rx = bus.subscribe();
    let mut needs_redraw = true;
    let mut last_timer_redraw = Instant::now();
    // Elapsed timers in AgentRow update every second — refresh at that cadence when idle.
    const TIMER_INTERVAL: Duration = Duration::from_secs(1);

    loop {
        // Drain all pending agent events; mark dirty on any arrival.
        loop {
            match rx.try_recv() {
                Ok(ev) => { state.handle_event(ev); needs_redraw = true; }
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
            terminal.draw(|f| draw(f, &mut state))?;
            needs_redraw = false;
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    (KeyCode::Char('?'), _) | (KeyCode::F(1), _) => {
                        state.show_help = !state.show_help;
                    }
                    (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                        let len = state.agents.len();
                        if len > 0 {
                            let i = state.table_state.selected().map(|i| (i + 1) % len).unwrap_or(0);
                            state.table_state.select(Some(i));
                        }
                    }
                    (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                        let len = state.agents.len();
                        if len > 0 {
                            let i = state.table_state.selected().map(|i| {
                                if i == 0 { len - 1 } else { i - 1 }
                            }).unwrap_or(0);
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

fn draw(f: &mut Frame, state: &mut AppState) {
    let size = f.size();

    // Root layout: header / table / logs / footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Length(3),  // stats bar
            Constraint::Min(8),     // agent table
            Constraint::Length(10), // log panel
            Constraint::Length(1),  // footer
        ])
        .split(size);

    draw_header(f, chunks[0], state);
    draw_stats(f, chunks[1], state);
    draw_agent_table(f, chunks[2], state);
    draw_log_panel(f, chunks[3], state);
    draw_footer(f, chunks[4], state);

    if state.show_help {
        draw_help_overlay(f, size);
    }
}

fn draw_header(f: &mut Frame, area: Rect, state: &AppState) {
    let running = state.agents.values().filter(|a| matches!(a.status,
        TaskStatus::Planning | TaskStatus::Implementing | TaskStatus::Testing | TaskStatus::Scoring
    )).count();
    let title = Line::from(vec![
        Span::styled("⛵ lopi", Style::default().fg(KONJO_PURPLE).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Konjo agent orchestrator", Style::default().fg(Color::DarkGray)),
        Span::raw("  │  "),
        Span::styled(format!("🤖 {running} running"), Style::default().fg(Color::Cyan)),
        Span::raw("  │  "),
        Span::styled(format!("⏱ {}", state.uptime()), Style::default().fg(Color::DarkGray)),
    ]);
    let p = Paragraph::new(title)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(KONJO_PURPLE)));
    f.render_widget(p, area);
}

fn draw_stats(f: &mut Frame, area: Rect, state: &AppState) {
    let running = state.agents.values().filter(|a| !matches!(a.status,
        TaskStatus::Queued | TaskStatus::Success { .. } | TaskStatus::Failed { .. } | TaskStatus::RolledBack
    )).count();

    let stats = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(area);

    let cards = [
        (format!("{}", running), "🔵 Running", Color::Cyan),
        (format!("{}", state.queued_count), "⏳ Queued", Color::Yellow),
        (format!("{}", state.succeeded), "✅ Done", Color::Green),
        (format!("{}", state.failed), "❌ Failed", Color::Red),
    ];

    for (i, (val, label, color)) in cards.iter().enumerate() {
        let p = Paragraph::new(Line::from(vec![
            Span::styled(val.as_str(), Style::default().fg(*color).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(*label, Style::default().fg(Color::DarkGray)),
        ]))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(KONJO_DIM)));
        f.render_widget(p, stats[i]);
    }
}

fn draw_agent_table(f: &mut Frame, area: Rect, state: &mut AppState) {
    let agents: Vec<AgentRow> = state.sorted_agents().into_iter().cloned().collect();
    let header = Row::new([
        Cell::from("ID").style(Style::default().fg(KONJO_PURPLE).add_modifier(Modifier::BOLD)),
        Cell::from("Goal").style(Style::default().fg(KONJO_PURPLE).add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().fg(KONJO_PURPLE).add_modifier(Modifier::BOLD)),
        Cell::from("Att").style(Style::default().fg(KONJO_PURPLE).add_modifier(Modifier::BOLD)),
        Cell::from("Score").style(Style::default().fg(KONJO_PURPLE).add_modifier(Modifier::BOLD)),
        Cell::from("Branch").style(Style::default().fg(KONJO_PURPLE).add_modifier(Modifier::BOLD)),
        Cell::from("Elapsed").style(Style::default().fg(KONJO_PURPLE).add_modifier(Modifier::BOLD)),
    ]).height(1).bottom_margin(0);

    let rows: Vec<Row> = agents.iter().map(|a: &AgentRow| {
        let (status_label, status_color) = a.status_label();
        let id_short = a.id.0.to_string()[..8].to_string();
        let goal = if a.goal.len() > 40 { format!("{}…", &a.goal[..39]) } else { a.goal.clone() };
        let branch = if a.branch.len() > 22 { format!("…{}", &a.branch[a.branch.len()-20..]) } else { a.branch.clone() };
        let score_pct = format!("{:.0}%", a.score * 100.0);

        Row::new([
            Cell::from(id_short).style(Style::default().fg(Color::DarkGray)),
            Cell::from(goal),
            Cell::from(status_label).style(Style::default().fg(status_color)),
            Cell::from(format!("{}/{}", a.attempt, a.max_retries)),
            Cell::from(score_pct).style(Style::default().fg(if a.score >= 1.0 { Color::Green } else { Color::Yellow })),
            Cell::from(branch).style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM)),
            Cell::from(a.elapsed()).style(Style::default().fg(Color::DarkGray)),
        ])
    }).collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Min(20),
        Constraint::Length(18),
        Constraint::Length(6),
        Constraint::Length(7),
        Constraint::Length(24),
        Constraint::Length(8),
    ];

    let filter_note = if state.log_filter.is_some() { " [log filtered]" } else { "" };
    let agent_count = agents.len();
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default()
            .title(format!(" Agents ({}){}  ↑↓/jk select  Enter filter logs  l toggle  ? help ", agent_count, filter_note))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(KONJO_DIM)))
        .highlight_style(Style::default().bg(Color::Rgb(40, 30, 60)).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(table, area, &mut state.table_state);
}

fn draw_log_panel(f: &mut Frame, area: Rect, state: &AppState) {
    let logs = state.visible_logs();
    let items: Vec<ListItem> = logs.iter().map(|entry| {
        let id_short = entry.task_id.0.to_string()[..6].to_string();
        let (prefix, color) = match entry.level {
            LogLevel::Info  => ("·", Color::White),
            LogLevel::Warn  => ("⚠", Color::Yellow),
            LogLevel::Error => ("✗", Color::Red),
            LogLevel::Debug => ("·", Color::DarkGray),
        };
        ListItem::new(Line::from(vec![
            Span::styled(format!("{id_short} "), Style::default().fg(KONJO_DIM)),
            Span::styled(format!("{prefix} "), Style::default().fg(color)),
            Span::styled(entry.line.clone(), Style::default().fg(color)),
        ]))
    }).collect();

    let filter_label = state.log_filter
        .map(|id| format!(" [{}…] ", &id.0.to_string()[..6]))
        .unwrap_or_default();
    let list = List::new(items)
        .block(Block::default()
            .title(format!(" Logs{} ", filter_label))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(KONJO_DIM)));
    f.render_widget(list, area);
}

fn draw_footer(f: &mut Frame, area: Rect, _state: &AppState) {
    let p = Paragraph::new(
        " q quit  ↑↓/jk navigate  Enter filter logs  l toggle filter  Esc clear  ? help",
    ).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(KONJO_PURPLE));
    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled("  Keyboard Controls", Style::default().add_modifier(Modifier::BOLD).fg(KONJO_PURPLE))]),
        Line::from(""),
        Line::from("  q / Ctrl-C    Quit"),
        Line::from("  ↑ / k         Select previous agent"),
        Line::from("  ↓ / j         Select next agent"),
        Line::from("  Enter          Filter logs to selected agent"),
        Line::from("  l              Toggle log filter"),
        Line::from("  Esc            Clear selection and filter"),
        Line::from("  ? / F1         Toggle this help"),
        Line::from(""),
        Line::from(vec![Span::styled("  Status Colors", Style::default().add_modifier(Modifier::BOLD).fg(KONJO_PURPLE))]),
        Line::from(""),
        Line::from(vec![Span::styled("  ⏳ Queued", Style::default().fg(Color::Yellow)), Span::raw("   📋 Planning"), Span::raw("   🔨 Implementing")]),
        Line::from(vec![Span::styled("  🧪 Testing", Style::default().fg(Color::Magenta)), Span::raw("  ✅ Done"), Span::styled("   ❌ Failed", Style::default().fg(Color::Red))]),
        Line::from(""),
        Line::from("  KONJO — Know · Outline · Nail · Justify · Optimize"),
        Line::from(""),
    ];
    let popup_area = centered_rect(60, 70, area);
    f.render_widget(Clear, popup_area);
    f.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: false }),
        popup_area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(layout[1])[1]
}
