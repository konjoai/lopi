//! `GET /api/models` — the Claude model/effort catalog the model and effort
//! dropdowns fetch from, instead of each maintaining its own hardcoded list.
//!
//! Proxies Anthropic's live `GET /v1/models` server-side (the browser/macOS
//! app never talks to Anthropic directly — no API key on the client, no CORS
//! story), cached in-process with a TTL since model catalogs change on the
//! order of weeks, not requests. Falls back to `lopi_core::fallback_models()`
//! on any failure (no `ANTHROPIC_API_KEY`, network error, non-2xx, bad JSON)
//! — this handler never returns an error to the caller; a stale/unreachable
//! Anthropic is exactly when the dropdown most needs *something* to show.

use super::AppState;
use axum::extract::State;
use axum::response::{IntoResponse, Json};
use lopi_core::ModelInfo;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// How long a fetched catalog is served before the next request triggers a
/// re-fetch.
const CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Anthropic API timeout — short, since a slow/unreachable Anthropic must not
/// stall the dropdown; the fallback list is one `?` away.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

struct Cached {
    fetched_at: Instant,
    models: Vec<ModelInfo>,
}

/// In-process TTL cache for the live model catalog. `Clone`-cheap (an `Arc`
/// inside) so it lives on `AppState` and is shared across every request
/// without re-fetching per-request.
#[derive(Clone)]
pub struct ModelsCache {
    inner: Arc<RwLock<Option<Cached>>>,
}

impl Default for ModelsCache {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }
}

impl ModelsCache {
    /// Serve the cached catalog if still fresh; otherwise fetch live from
    /// Anthropic, cache the result, and serve that. Never errors — a fetch
    /// failure logs a warning and falls back to `lopi_core::fallback_models()`
    /// (preferred over serving a possibly very-stale cache entry: the
    /// fallback list is curated to stay usable, so it's the more honest
    /// choice on failure).
    pub async fn get_or_refresh(&self) -> Vec<ModelInfo> {
        if let Some(models) = self.fresh().await {
            return models;
        }
        let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") else {
            tracing::warn!("ANTHROPIC_API_KEY not set, serving fallback model catalog");
            return lopi_core::fallback_models();
        };
        match fetch_live(&api_key).await {
            Ok(models) => {
                *self.inner.write().await = Some(Cached {
                    fetched_at: Instant::now(),
                    models: models.clone(),
                });
                models
            }
            Err(e) => {
                tracing::warn!("live model catalog fetch failed, serving fallback: {e:#}");
                lopi_core::fallback_models()
            }
        }
    }

    async fn fresh(&self) -> Option<Vec<ModelInfo>> {
        let guard = self.inner.read().await;
        let cached = guard.as_ref()?;
        (cached.fetched_at.elapsed() < CACHE_TTL).then(|| cached.models.clone())
    }
}

#[derive(Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Deserialize)]
struct AnthropicModel {
    id: String,
    display_name: String,
    #[serde(default)]
    capabilities: Option<AnthropicCapabilities>,
}

#[derive(Deserialize, Default)]
struct AnthropicCapabilities {
    /// Tier name → support info. Deliberately untyped (`serde_json::Value`)
    /// rather than a fixed `bool` or object shape: two consecutive live
    /// responses disagreed on which it is (`{"low": true}` in one, an object
    /// in another — possibly varying per tier or per model), so this parses
    /// either without erroring. See `effort_supported` for how a value is
    /// read.
    #[serde(default)]
    effort: HashMap<String, serde_json::Value>,
}

/// A tier counts as supported unless its value is the explicit JSON boolean
/// `false`. Covers `{"low": true}`, `{"low": {...detail...}}`, and any other
/// shape Anthropic sends for a tier it lists — presence in the map is the
/// primary signal, since the exact value shape has already proven to vary.
fn effort_supported(value: &serde_json::Value) -> bool {
    !matches!(value, serde_json::Value::Bool(false))
}

/// Call Anthropic's `GET /v1/models` and shape the response into
/// `ModelInfo`s, filtered to the Claude family. Errors on a transport/HTTP
/// error, a non-2xx status, unparseable JSON, or an empty/all-non-Claude
/// result — every case the caller treats identically (fall back to the
/// static list).
async fn fetch_live(api_key: &str) -> anyhow::Result<Vec<ModelInfo>> {
    let resp = reqwest::Client::new()
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .timeout(FETCH_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .json::<AnthropicModelsResponse>()
        .await?;

    let models: Vec<ModelInfo> = resp
        .data
        .into_iter()
        .filter(|m| m.id.starts_with("claude-"))
        .map(|m| {
            let effort = m
                .capabilities
                .map(|c| {
                    c.effort
                        .into_iter()
                        .filter(|(_, v)| effort_supported(v))
                        .map(|(name, _)| name)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            ModelInfo {
                id: m.id,
                display_name: m.display_name,
                effort,
            }
        })
        .collect();

    if models.is_empty() {
        anyhow::bail!("Anthropic returned zero Claude models");
    }
    Ok(models)
}

/// `GET /api/models` — the live-or-fallback Claude model/effort catalog.
pub async fn get_models(State(s): State<AppState>) -> impl IntoResponse {
    let models = s.models_cache.get_or_refresh().await;
    Json(json!({ "models": models }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks in the real Anthropic response shape — or rather, the *lack* of
    /// one fixed shape: two consecutive live fetches disagreed on whether an
    /// effort-tier value is a plain `bool` or a nested object (first
    /// `invalid type: boolean true, expected struct AnthropicEffortTier`,
    /// then — after switching to `HashMap<String, bool>` — `invalid type:
    /// map, expected a boolean`). This mixes both shapes, plus an explicit
    /// `false`, an empty map, and a model with no `capabilities` field at
    /// all, in one response, and asserts every case decodes and resolves
    /// correctly. No network call — a hardcoded sample body.
    #[test]
    fn anthropic_response_with_mixed_effort_value_shapes_decodes() -> anyhow::Result<()> {
        let body = r#"{
            "data": [
                {
                    "id": "claude-opus-4-8",
                    "display_name": "Claude Opus 4.8",
                    "capabilities": {
                        "effort": {
                            "low": true,
                            "medium": { "supported": true, "note": "nested object shape" },
                            "high": true,
                            "max": false
                        }
                    }
                },
                {
                    "id": "claude-haiku-4-5",
                    "display_name": "Claude Haiku 4.5",
                    "capabilities": { "effort": {} }
                },
                {
                    "id": "claude-3-opus-legacy",
                    "display_name": "Legacy Opus"
                }
            ]
        }"#;
        let parsed: AnthropicModelsResponse = serde_json::from_str(body)?;
        assert_eq!(parsed.data.len(), 3);
        let opus_caps = parsed.data[0].capabilities.as_ref().ok_or_else(|| {
            anyhow::anyhow!("sample body's first model must decode with capabilities")
        })?;
        let mut opus_supported: Vec<&str> = opus_caps
            .effort
            .iter()
            .filter(|(_, v)| effort_supported(v))
            .map(|(k, _)| k.as_str())
            .collect();
        opus_supported.sort_unstable();
        assert_eq!(
            opus_supported,
            ["high", "low", "medium"],
            "a bool-true tier and an object-shaped tier both count as supported; only the explicit false is excluded"
        );
        assert!(
            parsed.data[2].capabilities.is_none(),
            "a model with no capabilities field at all must not error"
        );
        Ok(())
    }

    #[tokio::test]
    async fn cache_starts_empty_and_fresh_returns_none() {
        let cache = ModelsCache::default();
        assert!(cache.fresh().await.is_none());
    }

    #[tokio::test]
    async fn cache_serves_a_fresh_entry_without_refetching() {
        let cache = ModelsCache::default();
        let models = lopi_core::fallback_models();
        *cache.inner.write().await = Some(Cached {
            fetched_at: Instant::now(),
            models: models.clone(),
        });
        assert_eq!(cache.fresh().await, Some(models));
    }

    #[tokio::test]
    async fn cache_treats_an_expired_entry_as_stale() {
        let cache = ModelsCache::default();
        *cache.inner.write().await = Some(Cached {
            // Well past CACHE_TTL — Instant subtraction is safe since this is
            // always in the past relative to `now()`.
            fetched_at: Instant::now() - (CACHE_TTL + Duration::from_secs(1)),
            models: lopi_core::fallback_models(),
        });
        assert!(cache.fresh().await.is_none());
    }
}
