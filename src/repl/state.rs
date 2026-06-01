//! REPL state types — shared across the REPL event loop and draw functions.
use std::collections::VecDeque;

use lopi_core::LopiConfig;

use super::input::InputWidget;
use super::slash::SlashDef;

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
    /// Top two rows of the ASCII logo — rendered in ocean cyan.
    SplashTop,
    /// Middle two rows of the ASCII logo — rendered in brand purple.
    SplashMid,
    /// Bottom two rows of the ASCII logo — rendered in sea-glass teal.
    SplashBot,
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
    pub autocomplete: Vec<&'static SlashDef>,
    pub mode: ReplMode,
    pub bypass: bool,
    pub session_cost_usd: f32,
    pub show_help: bool,
    /// Frame counter advanced by the animation tick — wraps at 255.
    pub anim_tick: u8,
}

impl ReplState {
    pub fn new(repo: &std::path::Path, model: &str, cfg: Option<&LopiConfig>) -> Self {
        let repo_name = crate::repo_detect::repo_display_name(repo);
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
            anim_tick: 0,
        };
        state.splash();
        state
    }

    pub fn push(&mut self, text: impl Into<String>, style: LineStyle) {
        self.output_lines.push_back(OutputLine {
            text: text.into(),
            style,
        });
        while self.output_lines.len() > 2000 {
            self.output_lines.pop_front();
        }
    }

    fn splash(&mut self) {
        let art = [
            (r"  ██╗      ██████╗ ██████╗ ██╗", LineStyle::SplashTop),
            (r"  ██║     ██╔═══██╗██╔══██╗██║", LineStyle::SplashTop),
            (r"  ██║     ██║   ██║██████╔╝██║", LineStyle::SplashMid),
            (r"  ██║     ██║   ██║██╔═══╝ ██║", LineStyle::SplashMid),
            (r"  ███████╗╚██████╔╝██║     ██║", LineStyle::SplashBot),
            (r"  ╚══════╝ ╚═════╝ ╚═╝     ╚═╝", LineStyle::SplashBot),
        ];
        for (line, style) in art {
            self.push(line, style);
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
pub enum ReplEvent {
    AgentLog { line: String, style: LineStyle },
    TaskDone { label: String, success: bool },
    CostAccrued(f32),
}
