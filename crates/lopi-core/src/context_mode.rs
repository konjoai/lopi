//! Finding #4 (Symbol Index) — how a worker session learns the shape of its
//! repo. Split out of `loop_config.rs` purely to keep that file under the
//! 500-line CI file-size gate, mirroring how `AutonomyLevel` already lives
//! in its own `autonomy.rs` and is re-exported from `loop_config` — no
//! behavioral difference from being inline.

use serde::{Deserialize, Serialize};

/// `Index` (the default) seeds the planning prompt with `lopi-index`'s
/// deterministic repo map (directory skeleton, public surface, most-
/// referenced symbols) and gives the session access to the
/// `lopi_find`/`lopi_read`/`lopi_refs`/`lopi_query` navigation tools —
/// context by pointer, per the sprint's brief. `Inject` reproduces this
/// codebase's pre-Finding-#4 behavior exactly: no repo map, no index tools,
/// the worker's own built-in Read/Grep/Glob is the only navigation it gets.
/// Named `Inject` to match the brief's `context.mode = "index" | "inject"`
/// A/B control even though (per `LEDGER.md`'s Finding #4 entry) this codebase
/// never had a literal file-content-injection site to compare against — the
/// name is the toggle's contract, not a claim about what it removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    /// Repo map + index tools seeded into the planning prompt (default).
    #[default]
    Index,
    /// No repo map, no index tools — this codebase's pre-Finding-#4 behavior.
    Inject,
}

impl ContextMode {
    /// The canonical snake_case tag (`"index"` / `"inject"`), matching serde.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Inject => "inject",
        }
    }

    /// Parse a mode from a case-insensitive tag. `None` for anything else.
    #[must_use]
    pub fn from_tag(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "index" => Some(Self::Index),
            "inject" => Some(Self::Inject),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::ContextMode;

    #[test]
    fn default_is_index() {
        assert_eq!(ContextMode::default(), ContextMode::Index);
    }

    #[test]
    fn tag_round_trips() {
        for mode in [ContextMode::Index, ContextMode::Inject] {
            assert_eq!(ContextMode::from_tag(mode.tag()), Some(mode));
        }
    }

    #[test]
    fn from_tag_is_case_insensitive_and_trims() {
        assert_eq!(ContextMode::from_tag(" INDEX \n"), Some(ContextMode::Index));
        assert_eq!(ContextMode::from_tag("Inject"), Some(ContextMode::Inject));
    }

    #[test]
    fn from_tag_rejects_unknown() {
        assert_eq!(ContextMode::from_tag("bogus"), None);
    }

    #[test]
    fn serde_uses_snake_case_tags() {
        assert_eq!(
            serde_json::to_string(&ContextMode::Index).unwrap(),
            "\"index\""
        );
        assert_eq!(
            serde_json::to_string(&ContextMode::Inject).unwrap(),
            "\"inject\""
        );
    }
}
