//! Concurrent tool registry with JSON persistence.
//!
//! Backing store is `HashMap<String, ToolSpec>` behind a `tokio::sync::RwLock`
//! — readers are cheap, writers serialise. After every mutation the registry
//! flushes to `registry_path` so a process restart picks up where it left off.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Declared parameters + behaviour of a tool an agent can call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSpec {
    /// Stable identifier — `lowercase-kebab-case` recommended. The runner
    /// looks tools up by this name; the agent's `tools` allowlist references
    /// the same string.
    pub name: String,
    /// Human-readable purpose. Included in the planning prompt verbatim.
    pub description: String,
    /// JSON Schema describing the call payload. The runner does not validate
    /// against this schema today (P1.4 schema validator is opt-in via
    /// `Task::output_schema`); it ships for downstream MCP-style discovery.
    pub parameters: serde_json::Value,
    /// Hard ceiling on a single invocation. Caller is responsible for
    /// enforcement — the registry only stores the contract.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Maximum retry attempts (excluding the initial call) on transient
    /// errors. Same caveat: registry advertises, caller enforces.
    #[serde(default)]
    pub retries: u8,
    /// ISO-8601 timestamp of last write. Set automatically by `register`.
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

const fn default_timeout_ms() -> u64 {
    30_000
}

impl ToolSpec {
    /// Construct a new spec with defaults for timeout / retries.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            timeout_ms: default_timeout_ms(),
            retries: 0,
            updated_at: Utc::now(),
        }
    }
}

/// Error variants returned by the registry.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Name was empty or contained whitespace.
    #[error("invalid tool name: {0}")]
    InvalidName(String),
    /// Caller passed something that isn't a JSON Schema object at the root.
    #[error("parameters must be a JSON object, got {0}")]
    InvalidParameters(String),
    /// Filesystem-related — disk persistence failed.
    #[error("registry I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON (de)serialisation failure.
    #[error("registry serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// JSON shape persisted to disk — versioned for forward-compat.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolRegistrySnapshot {
    /// Schema version. Bumped when the on-disk shape changes.
    #[serde(default = "default_version")]
    pub version: u32,
    /// All registered tools, keyed by name.
    pub tools: HashMap<String, ToolSpec>,
}

const fn default_version() -> u32 {
    1
}

/// Concurrent registry. Cheap to `clone()` — wraps an `Arc<RwLock<...>>`.
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    inner: Arc<RwLock<HashMap<String, ToolSpec>>>,
    registry_path: Arc<PathBuf>,
    /// Serializes `save_to_disk`'s snapshot-then-write-tmp-then-rename
    /// sequence. Without it, concurrent flushes race on the same tmp file:
    /// one call's rename can find the tmp file already moved by another,
    /// failing with `NotFound`.
    save_lock: Arc<tokio::sync::Mutex<()>>,
}

impl ToolRegistry {
    /// Build an empty in-memory registry persisted to `registry_path`. The
    /// file is created lazily on the first successful write — no I/O
    /// happens here.
    pub fn new(registry_path: impl AsRef<Path>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            registry_path: Arc::new(registry_path.as_ref().to_path_buf()),
            save_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Build a registry hydrated from `registry_path`. Missing file → empty.
    ///
    /// # Errors
    /// Returns `Err` if the file exists but cannot be read or parsed.
    pub async fn load(registry_path: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let path = registry_path.as_ref().to_path_buf();
        let map = if path.exists() {
            let bytes = tokio::fs::read(&path).await?;
            let snap: ToolRegistrySnapshot = if bytes.is_empty() {
                ToolRegistrySnapshot::default()
            } else {
                serde_json::from_slice(&bytes)?
            };
            snap.tools
        } else {
            HashMap::new()
        };
        Ok(Self {
            inner: Arc::new(RwLock::new(map)),
            registry_path: Arc::new(path),
            save_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    /// Register or overwrite a tool spec, then flush to disk.
    ///
    /// # Errors
    /// Returns [`RegistryError::InvalidName`] for an empty/whitespace name,
    /// [`RegistryError::InvalidParameters`] if `parameters` isn't a JSON
    /// object, or [`RegistryError::Io`]/[`RegistryError::Serde`] if disk
    /// flush fails.
    pub async fn register(&self, mut spec: ToolSpec) -> Result<(), RegistryError> {
        validate_name(&spec.name)?;
        if !spec.parameters.is_object() {
            return Err(RegistryError::InvalidParameters(format!(
                "expected object, got {}",
                json_kind(&spec.parameters)
            )));
        }
        spec.updated_at = Utc::now();
        {
            let mut guard = self.inner.write().await;
            guard.insert(spec.name.clone(), spec);
        }
        self.save_to_disk().await
    }

    /// Look up a tool by name. Cheap read; many concurrent calls allowed.
    pub async fn get(&self, name: &str) -> Option<ToolSpec> {
        self.inner.read().await.get(name).cloned()
    }

    /// Snapshot every registered tool. Returned `Vec` is not ordered.
    pub async fn list(&self) -> Vec<ToolSpec> {
        self.inner.read().await.values().cloned().collect()
    }

    /// Remove a tool. Returns `true` when an entry was actually removed,
    /// `false` when the name was unknown. Flushes to disk on success.
    pub async fn deregister(&self, name: &str) -> bool {
        let removed = {
            let mut guard = self.inner.write().await;
            guard.remove(name).is_some()
        };
        if removed {
            if let Err(e) = self.save_to_disk().await {
                tracing::warn!(error = %e, "tool registry: deregister flush failed");
            }
        }
        removed
    }

    /// Drop every entry. Useful for tests and CLI bulk-reset commands.
    /// Flushes to disk on success.
    ///
    /// # Errors
    /// Returns `Err` if the disk flush after clearing fails.
    pub async fn clear(&self) -> Result<(), RegistryError> {
        {
            let mut guard = self.inner.write().await;
            guard.clear();
        }
        self.save_to_disk().await
    }

    /// Persist current state to `registry_path`. Atomic on most filesystems
    /// because we write to a temp file then rename.
    ///
    /// # Errors
    /// I/O failure on the temp write, rename, or serde encoding.
    pub async fn save_to_disk(&self) -> Result<(), RegistryError> {
        let _guard = self.save_lock.lock().await;
        let snap = ToolRegistrySnapshot {
            version: default_version(),
            tools: self.inner.read().await.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&snap)?;
        let path = self.registry_path.as_ref().clone();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(())
    }

    /// File system location this registry persists to.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.registry_path.as_ref()
    }
}

/// Conventional location: `$LOPI_HOME/tool_registry.json` if set, else
/// `~/.lopi/tool_registry.json`. Falls back to `./tool_registry.json` only
/// if no home directory is resolvable.
#[must_use]
pub fn default_registry_path() -> PathBuf {
    if let Ok(p) = std::env::var("LOPI_HOME") {
        return PathBuf::from(p).join("tool_registry.json");
    }
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".lopi").join("tool_registry.json"))
        .unwrap_or_else(|_| PathBuf::from("tool_registry.json"))
}

fn validate_name(name: &str) -> Result<(), RegistryError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(RegistryError::InvalidName("empty".into()));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(RegistryError::InvalidName(format!(
            "contains whitespace: `{name}`"
        )));
    }
    Ok(())
}

fn json_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn temp_path() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tool_registry.json");
        (dir, path)
    }

    fn sample_spec(name: &str) -> ToolSpec {
        ToolSpec::new(
            name,
            "test tool",
            json!({"type": "object", "properties": {"q": {"type": "string"}}}),
        )
    }

    #[tokio::test]
    async fn register_and_get_round_trip() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        reg.register(sample_spec("search-docs")).await.unwrap();
        let got = reg.get("search-docs").await.unwrap();
        assert_eq!(got.name, "search-docs");
        assert_eq!(got.description, "test tool");
        assert!(got.parameters.is_object());
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        assert!(reg.get("missing").await.is_none());
    }

    #[tokio::test]
    async fn register_rejects_empty_name() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        let err = reg
            .register(ToolSpec::new("", "x", json!({})))
            .await
            .unwrap_err();
        assert!(matches!(err, RegistryError::InvalidName(_)));
    }

    #[tokio::test]
    async fn register_rejects_whitespace_name() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        let err = reg
            .register(ToolSpec::new("has space", "x", json!({})))
            .await
            .unwrap_err();
        assert!(matches!(err, RegistryError::InvalidName(_)));
    }

    #[tokio::test]
    async fn register_rejects_non_object_parameters() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        let err = reg
            .register(ToolSpec::new("ok-name", "x", json!("not an object")))
            .await
            .unwrap_err();
        assert!(matches!(err, RegistryError::InvalidParameters(_)));
    }

    #[tokio::test]
    async fn list_returns_every_tool() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        for n in ["a-tool", "b-tool", "c-tool"] {
            reg.register(sample_spec(n)).await.unwrap();
        }
        let mut names: Vec<_> = reg.list().await.into_iter().map(|t| t.name).collect();
        names.sort();
        assert_eq!(names, vec!["a-tool", "b-tool", "c-tool"]);
    }

    #[tokio::test]
    async fn deregister_returns_false_for_missing() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        assert!(!reg.deregister("missing").await);
    }

    #[tokio::test]
    async fn deregister_removes_and_returns_true() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        reg.register(sample_spec("removeme")).await.unwrap();
        assert!(reg.deregister("removeme").await);
        assert!(reg.get("removeme").await.is_none());
    }

    #[tokio::test]
    async fn persistence_round_trip_via_load() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        reg.register(sample_spec("alpha")).await.unwrap();
        reg.register(sample_spec("bravo")).await.unwrap();

        // Reopen — should hydrate from disk.
        let reopened = ToolRegistry::load(&path).await.unwrap();
        assert!(reopened.get("alpha").await.is_some());
        assert!(reopened.get("bravo").await.is_some());
        assert_eq!(reopened.list().await.len(), 2);
    }

    #[tokio::test]
    async fn load_missing_file_returns_empty_registry() {
        let (_dir, path) = temp_path();
        // Don't create anything — just load.
        let reg = ToolRegistry::load(&path).await.unwrap();
        assert!(reg.list().await.is_empty());
    }

    #[tokio::test]
    async fn re_register_updates_in_place() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        reg.register(ToolSpec::new("x", "first", json!({})))
            .await
            .unwrap();
        reg.register(ToolSpec::new("x", "second", json!({})))
            .await
            .unwrap();
        let got = reg.get("x").await.unwrap();
        assert_eq!(got.description, "second");
        // Only one entry — overwrite, not duplicate.
        assert_eq!(reg.list().await.len(), 1);
    }

    #[tokio::test]
    async fn clear_drops_every_entry() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        reg.register(sample_spec("a")).await.unwrap();
        reg.register(sample_spec("b")).await.unwrap();
        reg.clear().await.unwrap();
        assert!(reg.list().await.is_empty());
    }

    #[test]
    fn default_path_honours_lopi_home_env() {
        std::env::set_var("LOPI_HOME", "/tmp/lopi-test-home");
        let p = default_registry_path();
        assert!(p.ends_with("tool_registry.json"));
        assert!(p.starts_with("/tmp/lopi-test-home"));
        std::env::remove_var("LOPI_HOME");
    }

    #[tokio::test]
    async fn load_corrupt_json_errors() {
        let (_dir, path) = temp_path();
        tokio::fs::write(&path, b"{ this is not valid json")
            .await
            .unwrap();
        let err = ToolRegistry::load(&path).await.unwrap_err();
        assert!(matches!(err, RegistryError::Serde(_)));
    }

    #[tokio::test]
    async fn load_empty_file_returns_empty_registry() {
        let (_dir, path) = temp_path();
        tokio::fs::write(&path, b"").await.unwrap();
        let reg = ToolRegistry::load(&path).await.unwrap();
        assert!(reg.list().await.is_empty());
    }

    /// Concurrent registrations of distinct tools must all land — the
    /// write lock serialises mutations, so nothing should be lost under
    /// contention.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_registrations_all_persist() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        let handles: Vec<_> = (0..20)
            .map(|i| {
                let reg = reg.clone();
                tokio::spawn(async move { reg.register(sample_spec(&format!("tool-{i}"))).await })
            })
            .collect();
        for h in handles {
            h.await.unwrap().unwrap();
        }
        assert_eq!(reg.list().await.len(), 20);

        // The on-disk snapshot reflects the same final state.
        let reloaded = ToolRegistry::load(&path).await.unwrap();
        assert_eq!(reloaded.list().await.len(), 20);
    }

    /// Concurrent register/deregister racing on the same name must leave
    /// the registry in one consistent state, not a torn or duplicated one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_register_and_deregister_same_name_stays_consistent() {
        let (_dir, path) = temp_path();
        let reg = ToolRegistry::new(&path);
        reg.register(sample_spec("shared")).await.unwrap();

        let reg_a = reg.clone();
        let reg_b = reg.clone();
        let (r1, r2) = tokio::join!(
            tokio::spawn(async move { reg_a.register(sample_spec("shared")).await }),
            tokio::spawn(async move { reg_b.deregister("shared").await }),
        );
        r1.unwrap().unwrap();
        r2.unwrap();

        // Either the register-after-deregister or deregister-after-register
        // ordering won — both are valid outcomes of a race, but the map
        // must contain at most one "shared" entry either way.
        let count = reg
            .list()
            .await
            .iter()
            .filter(|t| t.name == "shared")
            .count();
        assert!(count <= 1, "no duplicate/torn entries under contention");
    }
}
