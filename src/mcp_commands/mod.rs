//! `lopi mcp-serve` — expose a curated slice of lopi's task/agent operations
//! as an MCP server over stdio, so a Claude Code plugin session can submit
//! and inspect tasks without leaving the chat.
//!
//! **State-sharing design (MCP-Serve-1 KT4).** This builds its own
//! standalone `AgentPool` + `TaskQueue` + dispatch loop in-process —
//! mirroring `sail_commands::run`'s wiring minus the HTTP listener, browser
//! auto-open, Telegram bot, and cron/quota warm-up (out of scope for the
//! curated tool set) — rather than reaching into an already-running `lopi
//! sail` process, which isn't possible cross-process for in-memory state
//! anyway. The one piece that *is* shared with a concurrently-running `lopi
//! sail` is the `MemoryStore` (SQLite): both open the same DB file, so
//! `lopi_list_tasks`/`lopi_get_task`/`lopi_get_logs`/`lopi_get_agent_dag`/
//! `lopi_get_stats` all reflect true durable history regardless of which
//! process a task was submitted through. Live dispatch is *not* shared — a
//! task submitted here is executed by this process's own pool, not a
//! separately-running `sail`'s. See `LEDGER.md` for the full write-up.

use anyhow::{Context, Result};
use lopi_core::{AgentEvent, EventBus, LopiConfig, PermissionMode, Priority, Task, TaskId};
use lopi_mcp::{McpResource, McpResourceContents, McpTool, ToolHandler};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use lopi_ui::web::{repos_handlers, AppState};
use serde_json::{json, Value};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{stdin, stdout, BufReader};

use crate::util::{db_path, expand_home};

/// `lopi_get_stack_status` — the MCPB-App-1 aggregating tool + its bound
/// `ui://` widget resource. Split out to keep this file under the 500-line
/// CI gate; see `stack_status.rs`'s module doc for why the tool exists.
mod stack_status;

/// Start `lopi mcp-serve`: build a standalone in-process orchestrator (own
/// pool + queue, shared SQLite store) and serve the curated tool set over
/// stdio until the peer closes the connection.
///
/// # Errors
/// Returns an error if the store can't be opened or the stdio transport
/// fails.
pub async fn serve(repo: PathBuf, max_agents: usize, cfg: Option<&LopiConfig>) -> Result<()> {
    let db = cfg.map_or_else(db_path, |c| expand_home(c.lopi.db_path.clone()));
    let store = MemoryStore::open(&db)
        .await
        .context("opening lopi store for mcp-serve")?;
    let bus: EventBus<AgentEvent> = EventBus::new(512);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(max_agents, repo.clone(), queue.clone(), bus.clone())
            .with_store(store.clone()),
    );

    let dispatch_pool = (*pool).clone();
    tokio::spawn(async move {
        if let Err(e) = dispatch_pool.run().await {
            tracing::error!("mcp-serve pool dispatch error: {e}");
        }
    });

    let state = AppState::new_with_repo(store, bus, queue, pool, None, repo);
    let handler = LopiToolHandler { state };

    tracing::info!("lopi mcp-serve: ready, awaiting JSON-RPC requests on stdin");
    let reader = BufReader::new(stdin());
    let writer = stdout();
    lopi_mcp::serve(&handler, reader, writer).await
}

/// The curated `ToolHandler` backing `lopi mcp-serve` — thin glue over the
/// same `AppState` the axum web routes read/write. Reimplements their
/// (already-thin) bodies directly rather than importing them: the axum
/// handler fns are `pub(super)` to `lopi-ui::web`, not meant for cross-crate
/// reuse, and each is a couple of store/pool calls plus JSON shaping.
struct LopiToolHandler {
    state: AppState,
}

impl ToolHandler for LopiToolHandler {
    fn tools(&self) -> Vec<McpTool> {
        tool_defs()
    }

    fn call(&self, name: &str, arguments: Value) -> impl Future<Output = Result<String>> + Send {
        let state = self.state.clone();
        let name = name.to_string();
        async move { dispatch(&state, &name, arguments).await }
    }

    fn resources(&self) -> Vec<McpResource> {
        stack_status::ui_resources()
    }

    fn read_resource(&self, uri: &str) -> impl Future<Output = Result<McpResourceContents>> + Send {
        let uri = uri.to_string();
        async move { stack_status::ui_resource_contents(&uri) }
    }
}

/// The curated tool set: Track A's seven, MCPB-App-1's
/// `lopi_get_stack_status`, and MCPB-App-3's `lopi_list_repos` /
/// `lopi_list_branches` (the widget's stack-loop-builder view needs real
/// dropdowns, not free-text). Not extended beyond that without a concrete
/// widget need — every additional tool is context budget spent on every turn
/// a plugin user has installed.
fn tool_defs() -> Vec<McpTool> {
    let task_id_prop = json!({
        "task_id": {
            "type": "string",
            "description": "Task UUID, or a unique prefix of one.",
        }
    });
    vec![
        McpTool {
            name: "lopi_submit_task".into(),
            description: "Submit a new agent task to lopi's orchestrator. Returns the queued task's id.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "Natural-language goal for the agent to accomplish.",
                    },
                    "repo": {
                        "type": "string",
                        "description": "Path to the git repository to work in. Defaults to the server's configured repo.",
                    },
                    "priority": {
                        "type": "string",
                        "enum": ["low", "normal", "high", "critical"],
                        "description": "Task priority. Defaults to normal.",
                    },
                    "branch": {
                        "type": "string",
                        "description": "Target branch, surfaced to the agent as a planning constraint (mirrors the web UI's stack-config branch field).",
                    },
                    "model": {
                        "type": "string",
                        "description": "Explicit worker-model override, e.g. \"claude-opus-4-7\". Defaults to lopi's own complexity-based selection.",
                    },
                    "effort": {
                        "type": "string",
                        "enum": ["low", "medium", "high", "xhigh", "max"],
                        "description": "Reasoning-effort level for the worker session.",
                    },
                    "permission_mode": {
                        "type": "string",
                        "enum": ["bypassPermissions", "auto", "acceptEdits", "dontAsk"],
                        "description": "How much the worker session may act on tool calls without a human prompt. Defaults to bypassPermissions.",
                    },
                    "max_iterations": {
                        "type": "integer",
                        "description": "Hard iteration ceiling for the retry loop, taking precedence over the repo's .lopi/loop.toml. 0 means unlimited. Omitted leaves the repo default.",
                    },
                },
                "required": ["goal"],
            }),
            meta: None,
        },
        McpTool {
            name: "lopi_list_tasks".into(),
            description: "List the most recent lopi tasks and their status.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
            meta: None,
        },
        McpTool {
            name: "lopi_get_task".into(),
            description: "Get one lopi task's status by id.".into(),
            input_schema: json!({ "type": "object", "properties": task_id_prop, "required": ["task_id"] }),
            meta: None,
        },
        McpTool {
            name: "lopi_cancel_task".into(),
            description: "Cancel a running or queued lopi task and delete it.".into(),
            input_schema: json!({ "type": "object", "properties": task_id_prop, "required": ["task_id"] }),
            meta: None,
        },
        McpTool {
            name: "lopi_get_logs".into(),
            description: "Get the historical log tail for one lopi task, oldest first.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task UUID, or a unique prefix of one." },
                    "n": { "type": "integer", "description": "Max lines to return (default 200)." },
                },
                "required": ["task_id"],
            }),
            meta: None,
        },
        McpTool {
            name: "lopi_get_agent_dag".into(),
            description: "Get the DAG-structured execution trace (nodes + edges) for one lopi task.".into(),
            input_schema: json!({ "type": "object", "properties": task_id_prop, "required": ["task_id"] }),
            meta: None,
        },
        McpTool {
            name: "lopi_get_stats".into(),
            description: "Get lopi's live stats: running/queued/succeeded/failed counts, uptime, and today's token/cost totals.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
            meta: None,
        },
        stack_status::tool_def(),
        McpTool {
            name: "lopi_list_repos".into(),
            description: "List the git repos lopi can dispatch to: the server's configured repo plus its siblings and any extra --repos. Backs the stack-loop-builder repo dropdown.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
            meta: None,
        },
        McpTool {
            name: "lopi_list_branches".into(),
            description: "List local git branches for a repo, plus its default (current HEAD) branch. Backs the stack-loop-builder branch dropdown.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "repo": {
                        "type": "string",
                        "description": "Repo path, as returned by lopi_list_repos. Defaults to the server's configured repo.",
                    },
                },
            }),
            meta: None,
        },
    ]
}

/// Route one `tools/call` to its implementation, serializing the result to
/// the text lopi-mcp wraps as the tool's content.
async fn dispatch(state: &AppState, name: &str, args: Value) -> Result<String> {
    let result = match name {
        "lopi_submit_task" => submit_task(state, &args).await?,
        "lopi_list_tasks" => list_tasks(state).await,
        "lopi_get_task" => get_task(state, &args).await?,
        "lopi_cancel_task" => cancel_task(state, &args).await?,
        "lopi_get_logs" => get_logs(state, &args).await?,
        "lopi_get_agent_dag" => get_agent_dag(state, &args).await?,
        "lopi_get_stats" => get_stats(state).await,
        "lopi_get_stack_status" => stack_status::get_stack_status(state).await,
        "lopi_list_repos" => repos_handlers::repos_json(state).await,
        "lopi_list_branches" => list_branches(state, &args).await?,
        other => anyhow::bail!("unknown tool: {other}"),
    };
    Ok(result.to_string())
}

/// Read a required string argument, erroring with the tool's own schema
/// context rather than a bare `None`.
fn required_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {key}"))
}

/// Mirrors `handlers::create_task`'s core: build a `Task`, submit it to the
/// pool. Covers the same "stack default config" fields the web UI's
/// `StackConfigPopover` edits (model/effort/repo/branch/permission_mode —
/// `autonomy` excluded since it's client-only there too, wired to nothing on
/// the server) plus `max_iterations` (the widget's iteration-pill field,
/// mirroring `StackCard.svelte`'s `card.maxIterations`). Skips the REST
/// route's remaining advanced fields (verifier/budget/gate/until/
/// acceptance/...) — out of scope for the curated v1 tool set.
async fn submit_task(state: &AppState, args: &Value) -> Result<Value> {
    let goal = required_str(args, "goal")?;
    let mut task = Task::new(goal.clone());
    if let Some(repo) = args.get("repo").and_then(Value::as_str) {
        task.repo_path = Some(PathBuf::from(repo));
    }
    task.priority = match args.get("priority").and_then(Value::as_str) {
        Some("low") => Priority::Low,
        Some("high") => Priority::High,
        Some("critical") => Priority::Critical,
        _ => Priority::Normal,
    };
    // Same encoding `cardToTaskPayload`/`paneSubmitPayload` use on the web
    // side: `CreateTaskRequest` has no dedicated branch field, so a target
    // branch reaches the planner as a constraint instead.
    if let Some(branch) = args.get("branch").and_then(Value::as_str) {
        let branch = branch.trim();
        if !branch.is_empty() {
            task.constraints.push(format!("Target branch: {branch}"));
        }
    }
    if let Some(model) = args.get("model").and_then(Value::as_str) {
        task.model = Some(model.to_string());
    }
    if let Some(effort) = args.get("effort").and_then(Value::as_str) {
        task.effort = Some(effort.to_string());
    }
    if let Some(mode) = args.get("permission_mode").and_then(Value::as_str) {
        task.permission_mode = PermissionMode::parse(mode)?;
    }
    if let Some(v) = args.get("max_iterations") {
        let n = v
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("max_iterations must be a non-negative integer"))?;
        task.max_iterations = Some(
            u8::try_from(n)
                .map_err(|_| anyhow::anyhow!("max_iterations must be between 0 and 255"))?,
        );
    }
    let task_id = task.id.0.to_string();
    let duplicate_of = state.pool.submit(task).await.map(|id| id.0.to_string());
    Ok(json!({
        "id": task_id,
        "goal": goal,
        "queued": duplicate_of.is_none(),
        "duplicate_of": duplicate_of,
    }))
}

/// Mirrors `handlers::list_tasks`.
async fn list_tasks(state: &AppState) -> Value {
    let rows = state.store.load_history(100).await.unwrap_or_default();
    let costs = state.store.task_costs().await.unwrap_or_default();
    let body: Vec<_> = rows
        .into_iter()
        .map(|t| {
            let cost = costs.get(&t.id).copied().unwrap_or(0.0);
            json!({
                "id": t.id, "goal": t.goal, "status": t.status,
                "created_at": t.created_at, "completed_at": t.completed_at,
                "client_ref": t.client_ref, "cost": cost,
            })
        })
        .collect();
    json!({ "tasks": body })
}

/// Mirrors `handlers::get_task`.
async fn get_task(state: &AppState, args: &Value) -> Result<Value> {
    let id = required_str(args, "task_id")?;
    let rows = state.store.load_history(500).await.unwrap_or_default();
    Ok(match rows.into_iter().find(|t| t.id.starts_with(&id)) {
        Some(t) => {
            let cost = state
                .store
                .task_costs()
                .await
                .unwrap_or_default()
                .get(&t.id)
                .copied()
                .unwrap_or(0.0);
            json!({
                "id": t.id, "goal": t.goal, "status": t.status,
                "created_at": t.created_at, "completed_at": t.completed_at,
                "client_ref": t.client_ref, "cost": cost,
            })
        }
        None => json!({ "error": "task not found" }),
    })
}

/// Mirrors `handlers::cancel_task`.
async fn cancel_task(state: &AppState, args: &Value) -> Result<Value> {
    let id = required_str(args, "task_id")?;
    let rows = state.store.load_history(500).await.unwrap_or_default();
    let Some(t) = rows.into_iter().find(|t| t.id.starts_with(&id)) else {
        return Ok(json!({ "error": "task not found" }));
    };
    let Ok(uuid) = t.id.parse::<uuid::Uuid>() else {
        return Ok(json!({ "error": "invalid id" }));
    };
    let task_id = TaskId(uuid);
    let cancelled = state.pool.cancel(&task_id).await;
    let deleted = match state.store.delete_task(&task_id).await {
        Ok(removed) => removed,
        Err(e) => {
            tracing::warn!(error = %e, task_id = %t.id, "delete_task failed");
            false
        }
    };
    Ok(json!({ "id": t.id, "cancelled": cancelled, "deleted": deleted }))
}

/// Mirrors `task_stream_handlers::get_logs`.
async fn get_logs(state: &AppState, args: &Value) -> Result<Value> {
    let id = required_str(args, "task_id")?;
    let n = args.get("n").and_then(Value::as_i64).unwrap_or(200);
    match state.store.task_exists(&id).await {
        Ok(true) => {}
        Ok(false) => return Ok(json!({ "error": "unknown task id", "task_id": id })),
        Err(e) => {
            tracing::warn!("task_exists failed: {e}");
            return Ok(json!({ "error": format!("{e:#}") }));
        }
    }
    match state.store.load_task_logs(&id, n).await {
        Ok(rows) => {
            let body: Vec<_> = rows
                .into_iter()
                .map(|r| {
                    json!({
                        "id": r.id, "task_id": r.task_id, "ts": r.ts,
                        "level": r.level, "line": r.line,
                    })
                })
                .collect();
            Ok(json!({ "task_id": id, "logs": body }))
        }
        Err(e) => {
            tracing::warn!("load_task_logs failed: {e}");
            Ok(json!({ "error": format!("{e:#}") }))
        }
    }
}

/// Mirrors `metrics_handlers::get_agent_dag`; both it and this call the shared
/// `lopi_memory::dag_graph_json` for the actual JSON shaping.
async fn get_agent_dag(state: &AppState, args: &Value) -> Result<Value> {
    let id = required_str(args, "task_id")?;
    match state.store.task_exists(&id).await {
        Ok(true) => {}
        Ok(false) => return Ok(json!({ "error": "unknown task id", "task_id": id })),
        Err(e) => {
            tracing::warn!("task_exists failed: {e}");
            return Ok(json!({ "error": format!("{e:#}") }));
        }
    }
    match state.store.load_dag_nodes(&id).await {
        Ok(rows) => Ok(lopi_memory::dag_graph_json(&id, &rows)),
        Err(e) => {
            tracing::warn!("agent dag query failed: {e}");
            Ok(json!({ "error": format!("{e:#}") }))
        }
    }
}

/// Mirrors `repos_handlers::list_branches`; `repo` defaults to the server's
/// configured repo when omitted, same as the REST route's empty-query case.
async fn list_branches(state: &AppState, args: &Value) -> Result<Value> {
    let repo = args.get("repo").and_then(Value::as_str).unwrap_or("");
    Ok(repos_handlers::branches_json(state, repo).await)
}

/// Mirrors `handlers::get_stats`.
async fn get_stats(state: &AppState) -> Value {
    let counts = state.store.status_counts().await.unwrap_or_else(|e| {
        tracing::warn!("status_counts query failed: {e}");
        Default::default()
    });
    let uptime_secs = state.pool.stats().uptime_secs;
    let (total_tokens_today, total_cost_usd_today) =
        state.store.daily_token_totals().await.unwrap_or_else(|e| {
            tracing::warn!("daily_token_totals query failed: {e}");
            (0, 0.0)
        });
    json!({
        "running": counts.running, "queued": counts.queued,
        "succeeded": counts.succeeded, "failed": counts.failed,
        "uptime_secs": uptime_secs,
        "total_tokens_today": total_tokens_today,
        "total_cost_usd_today": total_cost_usd_today,
    })
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

/// Regression coverage at the real `lopi_mcp` JSON-RPC surface (MCPB-App-2
/// Phase 2) — see the module doc there for why this is separate from
/// `mod_tests.rs`'s `dispatch()`-level tests.
#[cfg(test)]
#[path = "server_wire_tests.rs"]
mod server_wire_tests;
