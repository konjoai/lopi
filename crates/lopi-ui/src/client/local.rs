//! `LocalClient` — a [`TuiClient`] wrapping an in-process
//! `Arc<lopi_orchestrator::AgentPool>` + `MemoryStore`, for a TUI embedded
//! inside a running `lopi sail` process (no HTTP round trip).
//!
//! **Phase 3 landmine, resolved (not assumed):** `ChainScheduleManager` is
//! *not* reachable in-process outside the axum `AppState` today — it's
//! constructed only inside `AppState::new_with_repo`
//! (`crates/lopi-ui/src/web/mod.rs`) from an `AgentPool` clone + a
//! `MemoryStore`, and nothing exposes that instance (or the ingredients to
//! build an equivalent live one) to code outside the web layer. `sail
//! _commands::run` builds its own `AgentPool`/`MemoryStore` but hands them
//! straight into `serve_with_repo`, never returning them to the caller.
//! `LocalClient` *could* construct its own `ChainScheduleManager` from the
//! same `AgentPool`/`MemoryStore` it already holds (`ChainScheduleManager
//! ::new` is a public constructor), but doing so today would create a
//! *second*, independent chain scheduler running against the same store as
//! `lopi sail`'s — a correctness hazard, not a convenience. So: every chain
//! method below returns [`ClientError::Unsupported`] rather than silently
//! stubbing an empty list or spinning up a second scheduler. See
//! `LEDGER.md` for the full finding.
//!
//! **Also not wired to any real `lopi watch --local` path today.**
//! `lopi watch --local` (`src/task_commands.rs`) currently constructs a
//! brand-new, empty `EventBus` with no `AgentPool`/`MemoryStore` behind
//! it — `LocalClient` has nothing to attach to there yet. This type exists
//! for a future "embedded TUI inside `sail`" mode (out of scope for T0,
//! which builds no new widgets) that shares the same `pool`/`store` `sail`
//! already constructs, the same way `AppState` does.

use super::{ChainSummary, ClientError, TaskSummary, TuiClient};
use crate::web::types::CreateTaskRequest;
use async_trait::async_trait;
use lopi_core::{PlanDecision, Task, TaskId, TaskSource};
use lopi_memory::MemoryStore;
use lopi_orchestrator::AgentPool;
use std::sync::Arc;

/// An in-process [`TuiClient`] over a live `AgentPool` + `MemoryStore`.
pub struct LocalClient {
    pool: Arc<AgentPool>,
    store: MemoryStore,
}

impl LocalClient {
    /// Wrap an already-running pool and its store.
    #[must_use]
    pub fn new(pool: Arc<AgentPool>, store: MemoryStore) -> Self {
        Self { pool, store }
    }

    /// Resolve a full task id or unique id prefix against recent history,
    /// mirroring the web layer's `find_by_id_prefix` semantics
    /// (`crates/lopi-ui/src/web/handlers.rs`) so `LocalClient` and
    /// `RemoteClient` agree on ambiguous-prefix handling.
    async fn resolve_task_id(&self, id: &str) -> Result<TaskId, ClientError> {
        let rows = self
            .store
            .load_history(500)
            .await
            .map_err(|e| ClientError::Other(e.to_string()))?;
        let mut matches = rows.into_iter().filter(|t| t.id.starts_with(id));
        let Some(first) = matches.next() else {
            return Err(ClientError::NotFound(id.to_string()));
        };
        if matches.next().is_some() {
            return Err(ClientError::Conflict(
                "id prefix matches more than one task".to_string(),
            ));
        }
        first
            .id
            .parse::<uuid::Uuid>()
            .map(TaskId)
            .map_err(|e| ClientError::Other(format!("stored task id is not a valid uuid: {e}")))
    }
}

/// Build a [`Task`] from a [`CreateTaskRequest`] the same way the HTTP
/// handler does (`crates/lopi-ui/src/web/handlers.rs::create_task` +
/// `apply_loop_fields`) — every `Option<T>` field maps straight across,
/// `None` left as `Task::new`'s own default so unset fields still inherit
/// the repo's `.lopi/loop.toml`. Returns an error for the same two fields
/// the HTTP path validates (`report`, `permission_mode`) rather than
/// silently dropping an invalid value, so `LocalClient` and `RemoteClient`
/// agree on malformed input, not just the happy path.
fn request_to_task(request: &CreateTaskRequest) -> Result<Task, ClientError> {
    let mut task = Task::new(request.goal.clone());
    task.source = TaskSource::Api;

    task.priority = match request.priority.as_deref() {
        Some("low") => lopi_core::Priority::Low,
        Some("high") => lopi_core::Priority::High,
        Some("critical") => lopi_core::Priority::Critical,
        _ => lopi_core::Priority::Normal,
    };
    if let Some(repo) = &request.repo {
        task.repo_path = Some(std::path::PathBuf::from(repo));
    }
    if let Some(dirs) = &request.allowed_dirs {
        task.allowed_dirs = dirs.clone();
    }
    if let Some(dirs) = &request.forbidden_dirs {
        task.forbidden_dirs = dirs.clone();
    }
    if let Some(c) = &request.constraints {
        task.constraints = c.clone();
    }
    if let Some(r) = request.max_retries {
        task.max_retries = r;
    }
    task.require_plan_approval = request.require_plan_approval.unwrap_or(false);
    task.client_ref = request.client_ref.clone();

    if let Some(report) = &request.report {
        lopi_core::ReportChannel::parse(report)
            .map_err(|e| ClientError::Other(format!("invalid report channel: {e}")))?;
        task.report = Some(report.clone());
    }
    if let Some(mode) = &request.permission_mode {
        task.permission_mode = lopi_core::PermissionMode::parse(mode)
            .map_err(|e| ClientError::Other(format!("invalid permission_mode: {e}")))?;
    }
    if let Some(v) = request.verifier_required {
        task.verifier_required = v;
    }
    task.verifier_model = request.verifier_model.clone().or(task.verifier_model);
    task.verifier_effort = request.verifier_effort.clone().or(task.verifier_effort);
    if let Some(n) = request.max_iterations {
        task.max_iterations = Some(n);
    }
    if let Some(a) = request.autonomy_level {
        task.autonomy_level = Some(a);
    }
    if let Some(n) = request.no_progress_limit {
        task.no_progress_limit = Some(n);
    }
    if let Some(i) = request.isolation {
        task.isolation = Some(i);
    }
    if let Some(m) = &request.model {
        task.model = Some(m.clone());
    }
    if let Some(e) = &request.effort {
        task.effort = Some(e.clone());
    }
    if let Some(d) = request.deliverable {
        task.deliverable = Some(d);
    }
    if let Some(g) = &request.gate {
        task.gate = Some(g.clone());
    }
    if let Some(u) = &request.until {
        task.until = Some(u.clone());
    }
    if let Some(f) = request.on_fail {
        task.on_fail = Some(f);
    }
    if let Some(a) = &request.acceptance {
        task.acceptance = Some(a.clone());
    }
    if let Some(fo) = request.verifier_fail_open {
        task.verifier_fail_open = fo;
    }
    if let Some(b) = request.budget_tokens {
        task.budget_tokens = b;
    }
    if let Some(bo) = &request.budget_override {
        task.budget_override = Some(bo.clone());
    }
    Ok(task)
}

const CHAIN_METHODS_UNSUPPORTED: &str =
    "LocalClient has no reachable ChainScheduleManager — use RemoteClient against a running `lopi sail` for chain operations";

#[async_trait]
impl TuiClient for LocalClient {
    async fn list_tasks(&self) -> Result<Vec<TaskSummary>, ClientError> {
        let rows = self
            .store
            .load_history(200)
            .await
            .map_err(|e| ClientError::Other(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|t| TaskSummary {
                id: t.id,
                goal: t.goal,
                status: serde_json::Value::String(t.status),
                created_at: Some(t.created_at),
                completed_at: t.completed_at,
                client_ref: t.client_ref,
                cost: None,
                repo: t.repo,
                provenance: None,
            })
            .collect())
    }

    async fn get_task(&self, id: &str) -> Result<TaskSummary, ClientError> {
        let task_id = self.resolve_task_id(id).await?;
        let row = self
            .store
            .get_task(&task_id)
            .await
            .map_err(|e| ClientError::Other(e.to_string()))?
            .ok_or_else(|| ClientError::NotFound(id.to_string()))?;
        Ok(TaskSummary {
            id: row.id,
            goal: row.goal,
            status: serde_json::Value::String(row.status),
            created_at: Some(row.created_at),
            completed_at: row.completed_at,
            client_ref: row.client_ref,
            cost: None,
            repo: row.repo,
            provenance: None,
        })
    }

    async fn create_task(&self, request: &CreateTaskRequest) -> Result<String, ClientError> {
        let task = request_to_task(request)?;
        let id = task.id;
        let duplicate_of = self.pool.submit(task).await;
        Ok(duplicate_of.unwrap_or(id).0.to_string())
    }

    async fn cancel_task(&self, id: &str) -> Result<bool, ClientError> {
        Ok(self.pool.cancel_by_prefix(id).await)
    }

    async fn approve_plan(&self, id: &str) -> Result<(), ClientError> {
        let task_id = self.resolve_task_id(id).await?;
        if self.pool.decide_plan(&task_id, PlanDecision::Approve).await {
            Ok(())
        } else {
            Err(ClientError::Conflict("task is not awaiting plan approval".to_string()))
        }
    }

    async fn reject_plan(&self, id: &str) -> Result<(), ClientError> {
        let task_id = self.resolve_task_id(id).await?;
        if self.pool.decide_plan(&task_id, PlanDecision::Reject).await {
            Ok(())
        } else {
            Err(ClientError::Conflict("task is not awaiting plan approval".to_string()))
        }
    }

    async fn list_chains(&self) -> Result<Vec<ChainSummary>, ClientError> {
        Err(ClientError::Unsupported(CHAIN_METHODS_UNSUPPORTED.to_string()))
    }

    async fn get_chain(&self, _id: &str) -> Result<ChainSummary, ClientError> {
        Err(ClientError::Unsupported(CHAIN_METHODS_UNSUPPORTED.to_string()))
    }

    async fn create_chain(&self, _body: serde_json::Value) -> Result<ChainSummary, ClientError> {
        Err(ClientError::Unsupported(CHAIN_METHODS_UNSUPPORTED.to_string()))
    }

    async fn enable_chain(&self, _id: &str) -> Result<(), ClientError> {
        Err(ClientError::Unsupported(CHAIN_METHODS_UNSUPPORTED.to_string()))
    }

    async fn disable_chain(&self, _id: &str) -> Result<(), ClientError> {
        Err(ClientError::Unsupported(CHAIN_METHODS_UNSUPPORTED.to_string()))
    }

    async fn run_chain_now(&self, _id: &str) -> Result<(), ClientError> {
        Err(ClientError::Unsupported(CHAIN_METHODS_UNSUPPORTED.to_string()))
    }

    async fn get_loop_config(&self) -> Result<serde_json::Value, ClientError> {
        Err(ClientError::Unsupported(
            "LocalClient has no loop-engineering snapshot builder — use RemoteClient".to_string(),
        ))
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
