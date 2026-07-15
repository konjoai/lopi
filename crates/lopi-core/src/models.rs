//! Claude model catalog types. `GET /api/models` (in `lopi-ui`) proxies
//! Anthropic's live `GET /v1/models` and serves the fallback list here when
//! that call fails (no `ANTHROPIC_API_KEY`, network error, non-2xx, etc).
//!
//! This module owns the fallback list as the single Rust source of truth —
//! it replaces three independently hand-maintained copies that had already
//! drifted from each other (`lopi-agent::claude`'s `MODEL_*` constants stuck
//! on `claude-opus-4-7`, web's `options.ts`, and macOS's
//! `LaunchControls`/`StackConfigTypes`, both on `claude-opus-4-8`).

use serde::{Deserialize, Serialize};

/// One entry in the model catalog, whether served live from Anthropic or from
/// [`fallback_models`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The model ID passed as `--model` to the Claude CLI.
    pub id: String,
    /// Human-readable label for a dropdown (e.g. "Opus 4.8").
    pub display_name: String,
    /// Reasoning-effort tiers this model supports, low-to-high. Anthropic
    /// exposes this per-model via `capabilities.effort` on the live
    /// `/v1/models/{id}` response; the fallback list below hardcodes the
    /// tiers this repo's UI has offered historically.
    pub effort: Vec<String>,
}

impl ModelInfo {
    fn new(id: &str, display_name: &str, effort: &[&str]) -> Self {
        Self {
            id: id.to_string(),
            display_name: display_name.to_string(),
            effort: effort.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Served by `GET /api/models` when the live Anthropic call fails. Small and
/// deliberately not exhaustive — it exists so the model/effort dropdowns stay
/// usable offline or before the first successful live fetch, not to track
/// every model Anthropic ships.
pub fn fallback_models() -> Vec<ModelInfo> {
    const TIERS: &[&str] = &["low", "medium", "high", "max"];
    vec![
        ModelInfo::new("claude-opus-4-8", "Opus 4.8", TIERS),
        ModelInfo::new("claude-sonnet-5", "Sonnet 5", TIERS),
        ModelInfo::new("claude-sonnet-4-6", "Sonnet 4.6", TIERS),
        ModelInfo::new("claude-haiku-4-5", "Haiku 4.5", TIERS),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_list_is_non_empty_and_every_entry_has_effort_tiers() {
        let models = fallback_models();
        assert!(!models.is_empty(), "an empty fallback would leave the dropdown blank offline");
        for m in &models {
            assert!(!m.id.is_empty());
            assert!(!m.display_name.is_empty());
            assert!(!m.effort.is_empty(), "{} has no effort tiers", m.id);
        }
    }

    #[test]
    fn fallback_ids_are_unique() {
        let models = fallback_models();
        let mut ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), models.len(), "duplicate model id in the fallback list");
    }

    #[test]
    fn sonnet_5_is_present() {
        assert!(
            fallback_models().iter().any(|m| m.id == "claude-sonnet-5"),
            "the fallback list must include the current Sonnet — this is the whole point of centralizing it"
        );
    }

    #[test]
    fn round_trips_through_json() -> anyhow::Result<()> {
        for m in fallback_models() {
            let json = serde_json::to_string(&m)?;
            let back: ModelInfo = serde_json::from_str(&json)?;
            assert_eq!(back, m);
        }
        Ok(())
    }
}
