//! Tool profile — Sprint P1 (review-pipeline plan, Phase 1). Restricts which
//! tools an agent spawn may call, independent of two axes it sounds adjacent
//! to:
//! - [`crate::PermissionMode`] governs *how much* a session may act on a tool
//!   call without a human prompt (the CLI's own `--permission-mode`).
//! - [`crate::RepoProfile`] governs directory scope (`allowed_dirs`/
//!   `forbidden_dirs`), which Sprint P1's own entry-point audit found is
//!   advisory/post-hoc everywhere in this codebase, never a hard boundary —
//!   see `LEDGER.md`'s Review-Pipeline-Phase-1 entry.
//!
//! `ToolProfile` is a third, orthogonal axis: which tools *exist* for the
//! session at all, forced onto `--allowedTools` at the CLI boundary.
//!
//! Not the same system as `.claude/agents/*.md` frontmatter (`tools:`,
//! `permissionMode: plan`) — that's Claude Code subagent scope, unrelated to
//! `Task::tool_profile`. `researcher.md` uses `permissionMode: plan` there;
//! `PermissionMode::parse` rejects `"plan"` outright for this axis. Keep the
//! two systems distinct.

use serde::{Deserialize, Serialize};

/// Tool names allowed under [`ToolProfile::Readonly`]. Deny-by-default is
/// already [`crate::PermissionMode::DontAsk`]'s semantics ("only pre-approved
/// commands run; everything else is denied outright") — this profile is an
/// allow-list layered over that existing mechanism, not a new enforcement
/// path. Confirmed live (Sprint P1 PF-3 kill-test): a `DontAsk` session
/// restricted to this list, instructed to write a file, had the `Write` call
/// denied and terminated cleanly rather than stalling.
pub const READONLY_ALLOWED_TOOLS: &[&str] = &["Read", "Grep", "Glob", "WebFetch", "WebSearch"];

/// Restricts which tools a `claude -p` worker session may call at all.
/// Defaults to [`ToolProfile::Mutating`] so an absent field reproduces
/// pre-Sprint-P1 behavior exactly, the same default discipline
/// [`crate::PermissionMode`] uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToolProfile {
    /// Read/Grep/Glob/WebFetch/WebSearch only ([`READONLY_ALLOWED_TOOLS`]),
    /// forced under [`crate::PermissionMode::DontAsk`]. Used by a readonly
    /// Planner (Sprint P1 section 3): it can read the repo and the web but
    /// cannot write, edit, or run shell commands.
    #[serde(rename = "readonly")]
    Readonly,
    /// No restriction beyond whatever `permission_mode`/`allowed_tools`/
    /// `disallowed_tools` already apply. The default.
    #[default]
    #[serde(rename = "mutating")]
    Mutating,
}

impl ToolProfile {
    /// The permission mode this profile forces, if any. `Readonly` always
    /// forces `DontAsk` regardless of what the task itself requested — a
    /// readonly Planner does not get a more permissive posture just because
    /// a human or a trusted source asked for one. Mirrors
    /// [`crate::effective_permission_mode`]'s existing "the safer axis always
    /// wins" precedent for task-source trust; this is the same shape of
    /// override for a different trigger.
    #[must_use]
    pub const fn forced_permission_mode(self) -> Option<crate::PermissionMode> {
        match self {
            Self::Readonly => Some(crate::PermissionMode::DontAsk),
            Self::Mutating => None,
        }
    }

    /// The fixed `--allowedTools` list this profile forces, if any.
    /// `Readonly`'s list is authoritative: at the spawn site it replaces
    /// whatever `permission_allow` the task/repo configured rather than
    /// merging with it — a readonly spawn that also allowed one
    /// caller-supplied tool would make the whole profile decorative.
    #[must_use]
    pub fn forced_allowed_tools(self) -> Option<Vec<String>> {
        match self {
            Self::Readonly => Some(
                READONLY_ALLOWED_TOOLS
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            ),
            Self::Mutating => None,
        }
    }

    /// True for [`Self::Readonly`]. Convenience for call sites that only
    /// need the boolean, not the forced values.
    #[must_use]
    pub const fn is_readonly(self) -> bool {
        matches!(self, Self::Readonly)
    }
}

/// Combines this profile with the task-source/requested permission mode,
/// mirroring [`crate::effective_permission_mode`]'s "the safer axis always
/// wins" shape: `Readonly` always wins over whatever source-trust would have
/// produced, since [`crate::PermissionMode::DontAsk`] is already the
/// strictest of the four headless-safe modes. Extracted as a pure function
/// (not inlined at the one call site, `lopi-agent`'s `run_loop.rs`) so the
/// combination is unit-testable without a live CLI spawn, the same
/// discipline `effective_permission_mode` itself uses.
#[must_use]
pub fn effective_permission_mode_for_profile(
    profile: ToolProfile,
    source: &crate::TaskSource,
    requested: crate::PermissionMode,
) -> crate::PermissionMode {
    profile
        .forced_permission_mode()
        .unwrap_or_else(|| crate::effective_permission_mode(source, requested))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn default_is_mutating() {
        assert_eq!(ToolProfile::default(), ToolProfile::Mutating);
    }

    #[test]
    fn mutating_forces_nothing() {
        assert_eq!(ToolProfile::Mutating.forced_permission_mode(), None);
        assert_eq!(ToolProfile::Mutating.forced_allowed_tools(), None);
    }

    #[test]
    fn readonly_forces_dont_ask() {
        assert_eq!(
            ToolProfile::Readonly.forced_permission_mode(),
            Some(crate::PermissionMode::DontAsk)
        );
    }

    #[test]
    fn readonly_forces_exact_allow_list() {
        let tools = ToolProfile::Readonly.forced_allowed_tools().unwrap();
        assert_eq!(tools, vec!["Read", "Grep", "Glob", "WebFetch", "WebSearch"]);
    }

    #[test]
    fn readonly_allow_list_has_no_write_capable_tool() {
        let tools = ToolProfile::Readonly.forced_allowed_tools().unwrap();
        for forbidden in ["Write", "Edit", "MultiEdit", "NotebookEdit", "Bash", "Task"] {
            assert!(
                !tools.iter().any(|t| t == forbidden),
                "readonly allow-list must never include {forbidden}"
            );
        }
    }

    #[test]
    fn serde_wire_format_matches_permission_mode_style() {
        let readonly = serde_json::to_string(&ToolProfile::Readonly).unwrap();
        assert_eq!(readonly, "\"readonly\"");
        let mutating = serde_json::to_string(&ToolProfile::Mutating).unwrap();
        assert_eq!(mutating, "\"mutating\"");
    }

    #[test]
    fn is_readonly_helper() {
        assert!(ToolProfile::Readonly.is_readonly());
        assert!(!ToolProfile::Mutating.is_readonly());
    }

    #[test]
    fn readonly_wins_over_bypass_permissions_from_a_trusted_source() {
        let mode = effective_permission_mode_for_profile(
            ToolProfile::Readonly,
            &crate::TaskSource::Cli,
            crate::PermissionMode::BypassPermissions,
        );
        assert_eq!(mode, crate::PermissionMode::DontAsk);
    }

    #[test]
    fn readonly_wins_even_when_source_is_self_modify() {
        let mode = effective_permission_mode_for_profile(
            ToolProfile::Readonly,
            &crate::TaskSource::SelfModify {
                approved_by: "operator".into(),
            },
            crate::PermissionMode::BypassPermissions,
        );
        assert_eq!(mode, crate::PermissionMode::DontAsk);
    }

    #[test]
    fn mutating_defers_entirely_to_effective_permission_mode() {
        let mode = effective_permission_mode_for_profile(
            ToolProfile::Mutating,
            &crate::TaskSource::Cli,
            crate::PermissionMode::AcceptEdits,
        );
        assert_eq!(mode, crate::PermissionMode::AcceptEdits);
    }

    #[test]
    fn mutating_still_gets_untrusted_source_downgrade() {
        let mode = effective_permission_mode_for_profile(
            ToolProfile::Mutating,
            &crate::TaskSource::Webhook {
                repo: "org/repo".into(),
                event: "check_run".into(),
            },
            crate::PermissionMode::BypassPermissions,
        );
        assert_eq!(mode, crate::PermissionMode::DontAsk);
    }
}
