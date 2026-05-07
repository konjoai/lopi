use super::{AppState, AgentRow, LogLevel, KONJO_DIM, KONJO_PURPLE};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap,
    },
    Frame,
};

pub(super) fn draw(f: &mut Frame, state: &mut AppState) {
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
    use lopi_core::TaskStatus;
    let running = state
        .agents
        .values()
        .filter(|a| {
            matches!(
                a.status,
                TaskStatus::Planning
                    | TaskStatus::Implementing
                    | TaskStatus::Testing
                    | TaskStatus::Scoring
            )
        })
        .count();
    let title = Line::from(vec![
        Span::styled(
            "⛵ lopi",
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "Konjo agent orchestrator",
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("  │  "),
        Span::styled(
            format!("🤖 {running} running"),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  │  "),
        Span::styled(
            format!("⏱ {}", state.uptime()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let p = Paragraph::new(title).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(KONJO_PURPLE)),
    );
    f.render_widget(p, area);
}

fn draw_stats(f: &mut Frame, area: Rect, state: &AppState) {
    use lopi_core::TaskStatus;
    let running = state
        .agents
        .values()
        .filter(|a| {
            !matches!(
                a.status,
                TaskStatus::Queued
                    | TaskStatus::Success { .. }
                    | TaskStatus::Failed { .. }
                    | TaskStatus::RolledBack
            )
        })
        .count();

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(area);

    let cards = [
        (format!("{running}"), "🔵 Running", Color::Cyan),
        (
            format!("{}", state.queued_count),
            "⏳ Queued",
            Color::Yellow,
        ),
        (format!("{}", state.succeeded), "✅ Done", Color::Green),
        (format!("{}", state.failed), "❌ Failed", Color::Red),
    ];

    for (i, (val, label, color)) in cards.iter().enumerate() {
        let p = Paragraph::new(Line::from(vec![
            Span::styled(
                val.as_str(),
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(*label, Style::default().fg(Color::DarkGray)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(KONJO_DIM)),
        );
        f.render_widget(p, layout[i]);
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn draw_agent_table(f: &mut Frame, area: Rect, state: &mut AppState) {
    let agents: Vec<AgentRow> = state.sorted_agents().into_iter().cloned().collect();
    let header = Row::new([
        Cell::from("ID").style(
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Goal").style(
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Status").style(
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Att").style(
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Score").style(
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Branch").style(
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Elapsed").style(
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .height(1)
    .bottom_margin(0);

    let rows: Vec<Row> = agents
        .iter()
        .map(|a: &AgentRow| {
            let (status_label, status_color) = a.status_label();
            let id_short = a.id.0.to_string()[..8].to_string();
            let goal = if a.goal.len() > 40 {
                format!("{}…", &a.goal[..39])
            } else {
                a.goal.clone()
            };
            let branch = if a.branch.len() > 22 {
                format!("…{}", &a.branch[a.branch.len() - 20..])
            } else {
                a.branch.clone()
            };
            let score_pct = format!("{:.0}%", a.score * 100.0);

            Row::new([
                Cell::from(id_short).style(Style::default().fg(Color::DarkGray)),
                Cell::from(goal),
                Cell::from(status_label).style(Style::default().fg(status_color)),
                Cell::from(format!("{}/{}", a.attempt, a.max_retries)),
                Cell::from(score_pct).style(Style::default().fg(if a.score >= 1.0 {
                    Color::Green
                } else {
                    Color::Yellow
                })),
                Cell::from(branch).style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::DIM),
                ),
                Cell::from(a.elapsed()).style(Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Min(20),
        Constraint::Length(18),
        Constraint::Length(6),
        Constraint::Length(7),
        Constraint::Length(24),
        Constraint::Length(8),
    ];

    let filter_note = if state.log_filter.is_some() {
        " [log filtered]"
    } else {
        ""
    };
    let agent_count = agents.len();
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(format!(
                    " Agents ({agent_count}){filter_note}  ↑↓/jk select  Enter filter logs  l toggle  ? help "
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(KONJO_DIM)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 30, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(table, area, &mut state.table_state);
}

fn draw_log_panel(f: &mut Frame, area: Rect, state: &AppState) {
    let logs = state.visible_logs();
    let items: Vec<ListItem> = logs
        .iter()
        .map(|entry| {
            let id_short = entry.task_id.0.to_string()[..6].to_string();
            let (prefix, color) = match entry.level {
                LogLevel::Info => ("·", Color::White),
                LogLevel::Warn => ("⚠", Color::Yellow),
                LogLevel::Error => ("✗", Color::Red),
                LogLevel::Debug => ("·", Color::DarkGray),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{id_short} "), Style::default().fg(KONJO_DIM)),
                Span::styled(format!("{prefix} "), Style::default().fg(color)),
                Span::styled(entry.line.clone(), Style::default().fg(color)),
            ]))
        })
        .collect();

    let filter_label = state
        .log_filter
        .map(|id| format!(" [{}…] ", &id.0.to_string()[..6]))
        .unwrap_or_default();
    let list = List::new(items).block(
        Block::default()
            .title(format!(" Logs{filter_label} "))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(KONJO_DIM)),
    );
    f.render_widget(list, area);
}

fn draw_footer(f: &mut Frame, area: Rect, _state: &AppState) {
    let p = Paragraph::new(
        " q quit  ↑↓/jk navigate  Enter filter logs  l toggle filter  Esc clear  ? help",
    )
    .style(Style::default().fg(Color::DarkGray));
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
        Line::from(vec![Span::styled(
            "  Keyboard Controls",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(KONJO_PURPLE),
        )]),
        Line::from(""),
        Line::from("  q / Ctrl-C    Quit"),
        Line::from("  ↑ / k         Select previous agent"),
        Line::from("  ↓ / j         Select next agent"),
        Line::from("  Enter          Filter logs to selected agent"),
        Line::from("  l              Toggle log filter"),
        Line::from("  Esc            Clear selection and filter"),
        Line::from("  ? / F1         Toggle this help"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Status Colors",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(KONJO_PURPLE),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ⏳ Queued", Style::default().fg(Color::Yellow)),
            Span::raw("   📋 Planning"),
            Span::raw("   🔨 Implementing"),
        ]),
        Line::from(vec![
            Span::styled("  🧪 Testing", Style::default().fg(Color::Magenta)),
            Span::raw("  ✅ Done"),
            Span::styled("   ❌ Failed", Style::default().fg(Color::Red)),
        ]),
        Line::from(""),
        Line::from("  KONJO — Know · Outline · Nail · Justify · Optimize"),
        Line::from(""),
    ];
    let popup_area = centered_rect(60, 70, area);
    f.render_widget(Clear, popup_area);
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
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
