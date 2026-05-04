use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lopi_memory::MemoryStore;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::io;
use std::time::Duration;

/// Run the TUI dashboard until the user presses `q`.
pub async fn run(store: MemoryStore) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, store).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    res
}

async fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    store: MemoryStore,
) -> Result<()> {
    loop {
        let history = store.load_history(50).await.unwrap_or_default();
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
                .split(f.size());

            let header = Paragraph::new(Line::from(vec![
                Span::styled("🚢 lopi", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan)),
                Span::raw("  "),
                Span::raw("Konjo agent orchestrator — press q to quit"),
            ]))
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(header, chunks[0]);

            let items: Vec<ListItem> = history.iter().map(|t| {
                let line = format!("[{}] {} — {}", t.status, t.id.chars().take(8).collect::<String>(), t.goal);
                ListItem::new(line)
            }).collect();
            let list = List::new(items)
                .block(Block::default().title("Tasks").borders(Borders::ALL));
            f.render_widget(list, chunks[1]);

            let footer = Paragraph::new("KONJO — Know · Outline · Nail · Justify · Optimize")
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(footer, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(k) = event::read()? {
                if matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
                    return Ok(());
                }
            }
        }
    }
}
