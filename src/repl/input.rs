//! Minimal text-input widget for the lopi REPL — no external crate required.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Action returned by `InputWidget::handle_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    /// Nothing to do — keep polling.
    None,
    /// User pressed Enter; the goal string is ready to dispatch.
    Submit(String),
    /// Escape / Ctrl-C pressed while idle — caller should quit or cancel.
    Escape,
}

/// Single-line text input with cursor tracking and command history.
pub struct InputWidget {
    /// The raw text buffer, stored as characters for correct cursor indexing.
    buf: Vec<char>,
    /// Byte cursor position (index into `buf`).
    cursor: usize,
    /// Per-session input history (most recent last).
    history: Vec<String>,
    /// Current position when browsing history (`None` = not browsing).
    history_idx: Option<usize>,
    /// Snapshot of the live buffer taken when history browsing starts.
    history_draft: String,
}

impl InputWidget {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            cursor: 0,
            history: Vec::new(),
            history_idx: None,
            history_draft: String::new(),
        }
    }

    /// Current buffer contents as a `&str`-equivalent `String`.
    pub fn value(&self) -> String {
        self.buf.iter().collect()
    }

    /// Process a crossterm key event and return the resulting action.
    pub fn handle_key(&mut self, key: KeyEvent) -> InputAction {
        match (key.code, key.modifiers) {
            // Submit
            (KeyCode::Enter, _) => {
                let text = self.value();
                if text.is_empty() {
                    return InputAction::None;
                }
                self.buf.clear();
                self.cursor = 0;
                if self.history.last().map(|s| s.as_str()) != Some(&text) {
                    self.history.push(text.clone());
                }
                self.history_idx = None;
                InputAction::Submit(text)
            }

            // Escape / Ctrl-C
            (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.buf.clear();
                self.cursor = 0;
                self.history_idx = None;
                InputAction::Escape
            }

            // Clear line (Ctrl-U)
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.buf.clear();
                self.cursor = 0;
                self.history_idx = None;
                InputAction::None
            }

            // Cursor movement
            (KeyCode::Left, _) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                InputAction::None
            }
            (KeyCode::Right, _) => {
                if self.cursor < self.buf.len() {
                    self.cursor += 1;
                }
                InputAction::None
            }
            (KeyCode::Home, _) | (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                self.cursor = 0;
                InputAction::None
            }
            (KeyCode::End, _) | (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                self.cursor = self.buf.len();
                InputAction::None
            }

            // Deletion
            (KeyCode::Backspace, _) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.buf.remove(self.cursor);
                }
                InputAction::None
            }
            (KeyCode::Delete, _) => {
                if self.cursor < self.buf.len() {
                    self.buf.remove(self.cursor);
                }
                InputAction::None
            }

            // History
            (KeyCode::Up, _) => {
                if self.history.is_empty() {
                    return InputAction::None;
                }
                if self.history_idx.is_none() {
                    self.history_draft = self.value();
                }
                let new_idx = match self.history_idx {
                    None => self.history.len() - 1,
                    Some(0) => 0,
                    Some(i) => i - 1,
                };
                self.history_idx = Some(new_idx);
                let entry: Vec<char> = self.history[new_idx].chars().collect();
                self.cursor = entry.len();
                self.buf = entry;
                InputAction::None
            }
            (KeyCode::Down, _) => {
                match self.history_idx {
                    None => InputAction::None,
                    Some(i) if i + 1 < self.history.len() => {
                        let new_idx = i + 1;
                        self.history_idx = Some(new_idx);
                        let entry: Vec<char> = self.history[new_idx].chars().collect();
                        self.cursor = entry.len();
                        self.buf = entry;
                        InputAction::None
                    }
                    Some(_) => {
                        // Past the end — restore the draft
                        self.history_idx = None;
                        let draft: Vec<char> = self.history_draft.chars().collect();
                        self.cursor = draft.len();
                        self.buf = draft;
                        InputAction::None
                    }
                }
            }

            // Regular characters
            (KeyCode::Char(c), mods)
                if mods == KeyModifiers::NONE || mods == KeyModifiers::SHIFT =>
            {
                self.buf.insert(self.cursor, c);
                self.cursor += 1;
                InputAction::None
            }

            _ => InputAction::None,
        }
    }

    /// Render the input line into `area`. `prefix` is displayed before the cursor line.
    pub fn render(&self, f: &mut Frame, area: Rect, prefix: &str, active: bool) {
        let cursor_color = if active {
            Color::Yellow
        } else {
            Color::DarkGray
        };

        let before: String = self.buf[..self.cursor].iter().collect();
        let at_cursor = self.buf.get(self.cursor).copied().unwrap_or(' ');
        let after: String = self.buf[self.cursor + 1.min(self.buf.len() - self.cursor)..]
            .iter()
            .collect();

        let line = Line::from(vec![
            Span::styled(
                prefix,
                Style::default()
                    .fg(cursor_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(before),
            Span::styled(
                at_cursor.to_string(),
                Style::default()
                    .bg(cursor_color)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(after),
        ]);

        f.render_widget(Paragraph::new(line), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn type_and_submit() {
        let mut w = InputWidget::new();
        for ch in "hello".chars() {
            w.handle_key(key(KeyCode::Char(ch)));
        }
        assert_eq!(w.value(), "hello");
        let action = w.handle_key(key(KeyCode::Enter));
        assert_eq!(action, InputAction::Submit("hello".into()));
        assert_eq!(w.value(), "");
    }

    #[test]
    fn backspace_removes_last_char() {
        let mut w = InputWidget::new();
        for ch in "hi".chars() {
            w.handle_key(key(KeyCode::Char(ch)));
        }
        w.handle_key(key(KeyCode::Backspace));
        assert_eq!(w.value(), "h");
    }

    #[test]
    fn ctrl_u_clears() {
        let mut w = InputWidget::new();
        w.handle_key(key(KeyCode::Char('x')));
        w.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(w.value(), "");
    }

    #[test]
    fn empty_enter_is_noop() {
        let mut w = InputWidget::new();
        assert_eq!(w.handle_key(key(KeyCode::Enter)), InputAction::None);
    }
}
