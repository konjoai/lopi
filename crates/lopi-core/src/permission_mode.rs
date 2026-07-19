//! Permission mode — how much a `claude -p` worker session may act on tool
//! calls without a human answering a prompt, passed through as the CLI's own
//! `--permission-mode <mode>` flag.
//!
//! This is a genuinely different axis from two other lopi knobs that sound
//! adjacent:
//! - [`crate::task::Task::require_plan_approval`] is lopi's *own* gate — a
//!   channel-based human approval of the first attempt's plan, implemented
//!   in `lopi-agent::runner::plan_gate`. `PermissionMode` instead governs the
//!   CLI's own per-tool-call approval behavior for the whole session.
//! - [`crate::autonomy::AutonomyLevel`] governs PR/merge behavior after a
//!   run finishes (report-only … auto-merge), not execution-time tool
//!   permission at all.
//!
//! Only the four variants proven headless-safe by live kill-tests are
//! exposed here — `claude`'s own CLI also accepts `"manual"` and `"plan"`,
//! but both need every tool call to round-trip through a live human
//! decision, which headless `-p` has no channel for today (see
//! `docs/ops/LOPI_PERMISSION_MODES_SPRINT.md`'s Out of Scope section).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// How much a `claude -p` worker session may act on tool calls without a
/// human answering a prompt. Serializes to the exact literal strings the
/// CLI's `--permission-mode` flag accepts — not a snake_case translation
/// that would then need a lookup table at the spawn site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PermissionMode {
    /// No prompts, full autonomy — the CLI-equivalent of
    /// `--dangerously-skip-permissions` (confirmed a true drop-in, including
    /// the root/sudo refusal check, by this sprint's KT3). Default: an
    /// absent field must reproduce lopi's pre-existing unconditional
    /// behavior exactly.
    #[default]
    #[serde(rename = "bypassPermissions")]
    BypassPermissions,
    /// The model reviews each tool call and blocks anything it judges risky,
    /// aborting cleanly rather than stalling on a repeated classifier block
    /// (confirmed live by KT1).
    #[serde(rename = "auto")]
    Auto,
    /// File edits are auto-approved; every other tool call needs an
    /// allow-list entry (`Task::permission_allow` / `--allowedTools`) or it
    /// is denied, not stalled (confirmed live by KT2).
    #[serde(rename = "acceptEdits")]
    AcceptEdits,
    /// Only pre-approved commands run; everything else is denied outright,
    /// never stalled waiting on a prompt nothing in a headless pipeline can
    /// answer (confirmed live by KT1).
    #[serde(rename = "dontAsk")]
    DontAsk,
}

/// Why a `permission_mode` string failed to validate.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PermissionModeError {
    /// Not one of the four headless-safe modes this sprint exposes (which
    /// may still be a real CLI mode, e.g. `"plan"`/`"manual"` — those are
    /// deliberately not selectable here, see the module doc).
    #[error(
        "unknown permission mode `{0}` — valid modes: bypassPermissions, auto, acceptEdits, dontAsk"
    )]
    Unknown(String),
}

impl PermissionMode {
    /// Parse a wire-format `permission_mode` string. Exact match against the
    /// CLI's own literal flag values — never coerced or case-folded, since
    /// these come from a controlled dropdown, not free-form user text.
    ///
    /// # Errors
    /// Returns [`PermissionModeError::Unknown`] for anything other than the
    /// four accepted literals.
    pub fn parse(value: &str) -> Result<Self, PermissionModeError> {
        match value {
            "bypassPermissions" => Ok(Self::BypassPermissions),
            "auto" => Ok(Self::Auto),
            "acceptEdits" => Ok(Self::AcceptEdits),
            "dontAsk" => Ok(Self::DontAsk),
            other => Err(PermissionModeError::Unknown(other.to_string())),
        }
    }

    /// The literal string forwarded to `claude -p --permission-mode`, the
    /// inverse of [`Self::parse`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BypassPermissions => "bypassPermissions",
            Self::Auto => "auto",
            Self::AcceptEdits => "acceptEdits",
            Self::DontAsk => "dontAsk",
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn default_is_bypass_permissions() {
        assert_eq!(PermissionMode::default(), PermissionMode::BypassPermissions);
    }

    #[test]
    fn round_trips_as_str_for_every_variant() {
        for mode in [
            PermissionMode::BypassPermissions,
            PermissionMode::Auto,
            PermissionMode::AcceptEdits,
            PermissionMode::DontAsk,
        ] {
            assert_eq!(PermissionMode::parse(mode.as_str()), Ok(mode));
        }
    }

    #[test]
    fn as_str_matches_the_cli_literal_exactly() {
        assert_eq!(PermissionMode::BypassPermissions.as_str(), "bypassPermissions");
        assert_eq!(PermissionMode::Auto.as_str(), "auto");
        assert_eq!(PermissionMode::AcceptEdits.as_str(), "acceptEdits");
        assert_eq!(PermissionMode::DontAsk.as_str(), "dontAsk");
    }

    #[test]
    fn unknown_mode_names_itself_in_the_error() {
        let err = PermissionMode::parse("plan").unwrap_err();
        assert_eq!(err, PermissionModeError::Unknown("plan".to_string()));
        assert!(err.to_string().contains("plan"));
    }

    #[test]
    fn is_case_sensitive_not_coerced() {
        assert!(PermissionMode::parse("BypassPermissions").is_err());
        assert!(PermissionMode::parse("DONTASK").is_err());
    }

    #[test]
    fn serializes_to_the_cli_literal_not_snake_case() {
        let json = serde_json::to_string(&PermissionMode::AcceptEdits).unwrap();
        assert_eq!(json, "\"acceptEdits\"");
    }
}
