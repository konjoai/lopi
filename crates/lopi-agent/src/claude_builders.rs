//! Fluent `with_*` builders for [`ClaudeCode`](crate::claude::ClaudeCode),
//! split out of `claude.rs` purely to keep that file under the 500-line CI
//! file-size gate. Each setter takes `mut self` and returns `Self`; the
//! `ClaudeCode` fields are `pub(crate)` so these can set them directly from
//! this sibling module. No behavioral difference from being inline.

use crate::claude::ClaudeCode;
use std::time::Duration;

impl ClaudeCode {
    /// Override the Claude model used for CLI invocations.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the reasoning-effort level (`--effort`) for the worker session.
    /// Only the CLI's accepted levels (`low`/`medium`/`high`/`xhigh`/`max`,
    /// case-insensitive) are forwarded; an unrecognized value is dropped
    /// with a warning so a malformed task field can't make the CLI reject
    /// the whole spawn. `None`/empty leaves the CLI default in place.
    #[must_use]
    pub fn with_effort(mut self, effort: impl Into<String>) -> Self {
        let raw = effort.into();
        match crate::claude_support::normalize_effort(&raw) {
            Some(level) => self.effort = Some(level.to_string()),
            None if raw.trim().is_empty() => {}
            None => tracing::warn!(effort = %raw, "ignoring unrecognized effort level"),
        }
        self
    }

    /// Set the per-session `--max-turns` cap. The CLI halts cleanly at the cap
    /// and emits a terminal `result`, rather than running on.
    #[must_use]
    pub fn with_max_turns(mut self, turns: u32) -> Self {
        self.max_turns = Some(turns);
        self
    }

    /// Set the per-session `--max-budget-usd` cap. The CLI halts cleanly once
    /// cumulative cost reaches the cap.
    #[must_use]
    pub fn with_max_budget_usd(mut self, usd: f64) -> Self {
        self.max_budget_usd = Some(usd);
        self
    }

    /// Set `--allowedTools` — tool names explicitly permitted for this
    /// session. Empty (the default) adds nothing beyond the CLI's own
    /// defaults.
    #[must_use]
    pub fn with_allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = tools;
        self
    }

    /// Set `--disallowedTools` — tool names explicitly denied for this
    /// session. Empty (the default) denies nothing.
    #[must_use]
    pub fn with_disallowed_tools(mut self, tools: Vec<String>) -> Self {
        self.disallowed_tools = tools;
        self
    }

    /// Override the path to the `claude` CLI binary.
    #[must_use]
    pub fn with_cli(mut self, cli_path: impl Into<String>) -> Self {
        self.cli_path = cli_path.into();
        self
    }

    /// Set the per-invocation timeout in seconds.
    #[must_use]
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// Enable or disable `--output-format json` on CLI calls.
    #[must_use]
    pub fn with_json_output(mut self, enabled: bool) -> Self {
        self.json_output = enabled;
        self
    }

    /// Inject additional constraints from pattern memory into the planning prompt.
    #[must_use]
    pub fn with_extra_constraints(mut self, constraints: Vec<String>) -> Self {
        self.extra_constraints = constraints;
        self
    }

    /// Attach TOON-encoded keyword/constraint pattern pairs for the planning prompt.
    #[must_use]
    pub fn with_patterns(mut self, patterns: Vec<(String, String)>) -> Self {
        self.patterns = patterns;
        self
    }

    /// Attach lessons learned from past post-mortems for the planning prompt.
    #[must_use]
    pub fn with_lessons(mut self, lessons: Vec<(String, String)>) -> Self {
        self.lessons = lessons;
        self
    }
}
