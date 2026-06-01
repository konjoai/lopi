//! ratatui drawing functions for the Konjo interactive REPL.
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use super::{LineStyle, ReplMode, ReplState};
use crate::repl::slash::SLASH_COMMANDS;

const KONJO_PURPLE: Color = Color::Rgb(124, 58, 237);
const KONJO_DIM: Color = Color::Rgb(60, 60, 80);

/// Top-level draw entry point — builds the layout then delegates.
pub fn draw_repl(f: &mut Frame, state: &mut ReplState) {
    let has_completions = state.input.value().starts_with('/') && !state.autocomplete.is_empty();
    let autocomplete_height = if has_completions { 2u16 } else { 0 };

    let chunks = Layout::vertical([
        Constraint::Length(4),                   // header
        Constraint::Min(4),                      // output
        Constraint::Length(autocomplete_height), // autocomplete (collapsed when empty)
        Constraint::Length(3),                   // input
        Constraint::Length(1),                   // footer hint
    ])
    .split(f.size());

    draw_header(f, chunks[0], state);
    draw_output(f, chunks[1], state);
    if has_completions {
        draw_autocomplete(f, chunks[2], state);
    }
    draw_input_box(f, chunks[3], state);
    draw_footer(f, chunks[4], &state.mode);
}

fn draw_header(f: &mut Frame, area: Rect, state: &ReplState) {
    let (mode_text, mode_color) = match &state.mode {
        ReplMode::Idle => ("idle", Color::Green),
        ReplMode::Running => ("running", Color::Yellow),
    };

    let title_line = Line::from(vec![
        Span::styled(
            "  ⛵ lopi ",
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("· ", Style::default().fg(KONJO_DIM)),
        Span::styled(
            &state.repo_name,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  ", Style::default().fg(KONJO_DIM)),
        Span::styled(&state.model_short, Style::default().fg(Color::Cyan)),
        Span::styled("  ·  ", Style::default().fg(KONJO_DIM)),
        Span::styled(mode_text, Style::default().fg(mode_color)),
        if state.bypass {
            Span::styled(
                "  ⚠ bypass",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw("")
        },
    ]);

    let cost_line = Line::from(vec![
        Span::styled("  session cost: ", Style::default().fg(KONJO_DIM)),
        Span::styled(
            format!("${:.4}", state.session_cost_usd),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("  ·  type a goal or /help", Style::default().fg(KONJO_DIM)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(KONJO_PURPLE))
        .title(Span::styled(
            " Konjo ",
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Left);

    let inner = block.inner(area);
    f.render_widget(block, area);
    let inner_chunks =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    f.render_widget(Paragraph::new(title_line), inner_chunks[0]);
    f.render_widget(Paragraph::new(cost_line), inner_chunks[1]);
}

fn draw_output(f: &mut Frame, area: Rect, state: &ReplState) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::TOP)
        .border_style(Style::default().fg(KONJO_DIM))
        .title(Span::styled(" output ", Style::default().fg(KONJO_DIM)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Compute which lines are visible given scroll offset and height.
    let visible_height = inner.height as usize;
    let total = state.output_lines.len();
    let start = if total > visible_height + state.scroll_offset {
        total - visible_height - state.scroll_offset
    } else {
        0
    };
    let end = (start + visible_height).min(total);

    let items: Vec<ListItem> = state
        .output_lines
        .iter()
        .skip(start)
        .take(end - start)
        .map(|ol| {
            let style = match ol.style {
                LineStyle::Normal => Style::default().fg(Color::White),
                LineStyle::Success => Style::default().fg(Color::Green),
                LineStyle::Error => Style::default().fg(Color::Red),
                LineStyle::Info => Style::default().fg(Color::Cyan),
                LineStyle::AgentLog => Style::default().fg(Color::DarkGray),
                LineStyle::Splash => Style::default()
                    .fg(KONJO_PURPLE)
                    .add_modifier(Modifier::BOLD),
                LineStyle::Hint => Style::default().fg(KONJO_DIM),
            };
            ListItem::new(Line::from(Span::styled(ol.text.clone(), style)))
        })
        .collect();

    let list = List::new(items).style(Style::default().fg(Color::White));
    f.render_widget(list, inner);

    // Scroll indicator if there's content above.
    if state.scroll_offset > 0 {
        let indicator = Paragraph::new("▲ more above — PgUp/PgDn to scroll")
            .style(Style::default().fg(KONJO_DIM))
            .alignment(Alignment::Right)
            .wrap(Wrap { trim: true });
        let indicator_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        f.render_widget(indicator, indicator_area);
    }
}

fn draw_autocomplete(f: &mut Frame, area: Rect, state: &ReplState) {
    let prefix = state.input.value();
    let prefix_bare = prefix.strip_prefix('/').unwrap_or("");
    let name_only = prefix_bare
        .split_once(char::is_whitespace)
        .map(|(n, _)| n)
        .unwrap_or(prefix_bare);

    let mut spans: Vec<Span> = Vec::new();
    for def in &state.autocomplete {
        if def.name == name_only {
            spans.push(Span::styled(
                format!("  /{} ", def.name),
                Style::default()
                    .fg(KONJO_PURPLE)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!("— {} ", def.description),
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::styled(
                format!("  /{} ", def.name),
                Style::default().fg(Color::Cyan),
            ));
        }
    }

    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(KONJO_DIM));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn draw_input_box(f: &mut Frame, area: Rect, state: &mut ReplState) {
    let active = matches!(state.mode, ReplMode::Idle);
    let border_color = if active { KONJO_PURPLE } else { KONJO_DIM };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let prompt = format!("{} ❯ ", state.repo_name);
    let input_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    state.input.render(f, input_area, &prompt, active);
}

fn draw_footer(f: &mut Frame, area: Rect, mode: &ReplMode) {
    let hint = match mode {
        ReplMode::Idle => "Enter: run goal  /help: commands  PgUp/PgDn: scroll  Ctrl-C: quit",
        ReplMode::Running => "Agent running…  Ctrl-C: cancel task",
    };
    let p = Paragraph::new(Span::styled(hint, Style::default().fg(KONJO_DIM)));
    f.render_widget(p, area);
}

/// Draw a compact help overlay listing all slash commands.
pub fn draw_help_overlay(f: &mut Frame) {
    let area = centered_rect(70, 80, f.size());

    // Clear the background
    f.render_widget(ratatui::widgets::Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(KONJO_PURPLE))
        .title(Span::styled(
            " /help — Konjo REPL commands ",
            Style::default()
                .fg(KONJO_PURPLE)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for def in SLASH_COMMANDS {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:18}", def.usage),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(def.description, Style::default().fg(Color::White)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Keyboard: ", Style::default().fg(KONJO_DIM)),
        Span::raw("↑↓ history  Ctrl-U clear  PgUp/PgDn scroll  Esc cancel"),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Goal:     ", Style::default().fg(KONJO_DIM)),
        Span::raw("Type anything without / to run it as an agent task"),
    ]));

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vert = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vert[1])[1]
}
