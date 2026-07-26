//! Sprint F2 Phase 4 — externalized worker model IDs.
//!
//! `MODEL_HAIKU`/`MODEL_SONNET`/`MODEL_OPUS` used to be hardcoded Rust
//! constants (`claude_model.rs`), pinned two generations stale in code
//! (`claude-opus-4-7`) and a third generation stale in CI
//! (`claude-opus-4-6`, `.github/workflows/konjo-gate.yml`'s G5 review
//! header). This module makes the mapping from complexity tier to model ID
//! a runtime-read config, mirroring [`crate::pricing`]'s pattern exactly —
//! same override locations, same "bundled default, optional partial
//! override" shape.
//!
//! **Pinning semantics differ by generation** — see `models.toml`'s own doc
//! comment: from the 4.6 generation onward a dateless ID is a fixed
//! snapshot, not an alias, so this config schema does not need to assume
//! one pinning strategy for every tier.

use serde::Deserialize;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The three worker-model tiers `select_model` routes a task to.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelIds {
    /// Lowest-cost, fastest tier — read-only discovery, simple rewrites.
    #[serde(default)]
    pub haiku: Option<String>,
    /// Default balanced tier — implementation, test writing.
    #[serde(default)]
    pub sonnet: Option<String>,
    /// Highest-capability tier — complex multi-file changes, escalated retries.
    #[serde(default)]
    pub opus: Option<String>,
}

/// Compiled-in default model IDs, always available even with no override
/// file on disk.
const DEFAULT_MODELS_TOML: &str = include_str!("../models.toml");

fn parse_or_warn(text: &str, source: &str) -> ModelIds {
    match toml::from_str::<ModelIds>(text) {
        Ok(ids) => ids,
        Err(err) => {
            tracing::warn!(source, %err, "model config file failed to parse — ignoring");
            ModelIds {
                haiku: None,
                sonnet: None,
                opus: None,
            }
        }
    }
}

/// Operator override locations, repo-level first: `<repo>/.lopi/models.toml`
/// then `~/.lopi/models.toml`. Either may set only the tiers it wants to
/// change — tiers it omits keep the compiled-in default.
fn override_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from(".lopi/models.toml")];
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join(".lopi/models.toml"));
    }
    candidates
}

static RESOLVED: OnceLock<ModelIds> = OnceLock::new();

/// The effective model IDs: compiled-in defaults with any operator override
/// file's tiers layered on top. Read once and cached — lopi is a long-lived
/// process, so a rate/model-ID change on disk takes a restart to apply, the
/// same as any other config file it reads at startup.
fn resolved() -> &'static ModelIds {
    RESOLVED.get_or_init(|| {
        let mut ids = parse_or_warn(DEFAULT_MODELS_TOML, "bundled default models.toml");
        for path in override_candidates() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let overrides = parse_or_warn(&text, &path.display().to_string());
                if overrides.haiku.is_some() {
                    ids.haiku = overrides.haiku;
                }
                if overrides.sonnet.is_some() {
                    ids.sonnet = overrides.sonnet;
                }
                if overrides.opus.is_some() {
                    ids.opus = overrides.opus;
                }
                tracing::info!(path = %path.display(), "loaded model config override");
            }
        }
        ids
    })
}

/// The Haiku-tier worker model ID — lowest cost, fastest latency.
#[must_use]
pub fn model_haiku() -> &'static str {
    resolved().haiku.as_deref().unwrap_or(FALLBACK_HAIKU)
}

/// The Sonnet-tier worker model ID — default balanced model.
#[must_use]
pub fn model_sonnet() -> &'static str {
    resolved().sonnet.as_deref().unwrap_or(FALLBACK_SONNET)
}

/// The Opus-tier worker model ID — highest capability, used for complex or
/// retried tasks.
#[must_use]
pub fn model_opus() -> &'static str {
    resolved().opus.as_deref().unwrap_or(FALLBACK_OPUS)
}

/// Hard-coded last-resort fallback if `models.toml` itself is somehow
/// unparseable at build time — should never trigger in practice since the
/// bundled file is checked in and tested (`bundled_default_covers_all_three_tiers`).
const FALLBACK_HAIKU: &str = "claude-haiku-4-5-20251001";
const FALLBACK_SONNET: &str = "claude-sonnet-5";
const FALLBACK_OPUS: &str = "claude-opus-5";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_default_covers_all_three_tiers() {
        let ids = parse_or_warn(DEFAULT_MODELS_TOML, "test");
        assert!(ids.haiku.is_some());
        assert!(ids.sonnet.is_some());
        assert!(ids.opus.is_some());
    }

    #[test]
    fn model_accessors_return_the_bundled_default_when_no_override_present() {
        // `resolved()` is process-cached; in a test binary with no
        // `.lopi/models.toml` on the CWD or `$HOME`, it resolves to the
        // bundled default, matching this file's own values.
        assert_eq!(model_haiku(), "claude-haiku-4-5-20251001");
        assert_eq!(model_sonnet(), "claude-sonnet-5");
        assert_eq!(model_opus(), "claude-opus-5");
    }

    #[test]
    fn override_file_may_set_only_a_subset_of_tiers() {
        let overrides = parse_or_warn("opus = \"claude-opus-4-8\"\n", "test override");
        assert_eq!(overrides.opus.as_deref(), Some("claude-opus-4-8"));
        assert!(overrides.haiku.is_none());
        assert!(overrides.sonnet.is_none());
    }
}
