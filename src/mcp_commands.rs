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
use lopi_core::{AgentEvent, EventBus, LopiConfig, Priority, Task, TaskId};
use lopi_mcp::{McpTool, ToolHandler};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use lopi_ui::web::AppState;
use serde_json::{json, Value};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{stdin, stdout, BufReader};

use crate::util::{db_path, expand_home};

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
}

/// The curated tool set — the plan's Track A 1.1 table, exactly seven tools.
/// Deliberately not extended: every additional tool is context budget spent
/// on every turn a plugin user has installed.
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
                },
                "required": ["goal"],
            }),
        },
        McpTool {
            name: "lopi_list_tasks".into(),
            description: "List the most recent lopi tasks and their status.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        McpTool {
            name: "lopi_get_task".into(),
            description: "Get one lopi task's status by id.".into(),
            input_schema: json!({ "type": "object", "properties": task_id_prop, "required": ["task_id"] }),
        },
        McpTool {
            name: "lopi_cancel_task".into(),
            description: "Cancel a running or queued lopi task and delete it.".into(),
            input_schema: json!({ "type": "object", "properties": task_id_prop, "required": ["task_id"] }),
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
        },
        McpTool {
            name: "lopi_get_agent_dag".into(),
            description: "Get the DAG-structured execution trace (nodes + edges) for one lopi task.".into(),
            input_schema: json!({ "type": "object", "properties": task_id_prop, "required": ["task_id"] }),
        },
        McpTool {
            name: "lopi_get_stats".into(),
            description: "Get lopi's live stats: running/queued/succeeded/failed counts, uptime, and today's token/cost totals.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
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
/// pool. Skips the REST route's advanced fields (verifier/budget/permission
/// mode/...) — out of scope for the curated v1 tool set.
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A fresh in-memory-store `AppState` with its own pool/queue, no
    /// dispatch loop spawned — submitted tasks stay queued (never picked up
    /// by a runner), which is exactly what lets these tests inspect queue
    /// state directly instead of racing a real `AgentRunner`.
    async fn test_state() -> AppState {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let bus: EventBus<AgentEvent> = EventBus::new(8);
        let queue = TaskQueue::new();
        let pool = Arc::new(
            AgentPool::new(1, PathBuf::from("."), queue.clone(), bus.clone())
                .with_store(store.clone()),
        );
        AppState::new_with_repo(store, bus, queue, pool, None, PathBuf::from("."))
    }

    #[test]
    fn tool_defs_advertises_exactly_the_curated_seven() {
        let tools = tool_defs();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "lopi_submit_task",
                "lopi_list_tasks",
                "lopi_get_task",
                "lopi_cancel_task",
                "lopi_get_logs",
                "lopi_get_agent_dag",
                "lopi_get_stats",
            ]
        );
    }

    #[test]
    fn required_str_returns_the_actual_value() {
        let args = json!({ "task_id": "abc123" });
        assert_eq!(required_str(&args, "task_id").unwrap(), "abc123");
    }

    #[test]
    fn required_str_errors_on_missing_key() {
        assert!(required_str(&json!({}), "task_id").is_err());
    }

    #[tokio::test]
    async fn submit_task_queues_with_default_priority() {
        let state = test_state().await;
        let resp = submit_task(&state, &json!({ "goal": "default priority goal" }))
            .await
            .unwrap();
        assert_eq!(resp["goal"], "default priority goal");
        assert_eq!(resp["queued"], true);
        assert!(resp["id"].as_str().is_some());
        let queued = state.queue.peek_queued();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].0, Priority::Normal);
    }

    #[tokio::test]
    async fn submit_task_honors_each_priority_value() {
        let state = test_state().await;
        for (input, expected) in [
            ("low", Priority::Low),
            ("high", Priority::High),
            ("critical", Priority::Critical),
        ] {
            let goal = format!("{input} priority goal");
            submit_task(&state, &json!({ "goal": goal, "priority": input }))
                .await
                .unwrap();
            let queued = state.queue.peek_queued();
            let entry = queued
                .iter()
                .find(|(_, g)| g == &goal)
                .expect("submitted goal missing from queue");
            assert_eq!(entry.0, expected, "priority {input} mapped incorrectly");
        }
    }

    #[tokio::test]
    async fn submit_task_requires_goal() {
        let state = test_state().await;
        assert!(submit_task(&state, &json!({})).await.is_err());
    }

    #[tokio::test]
    async fn list_tasks_reflects_a_submitted_task() {
        let state = test_state().await;
        submit_task(&state, &json!({ "goal": "listed goal" }))
            .await
            .unwrap();
        let resp = list_tasks(&state).await;
        let tasks = resp["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["goal"], "listed goal");
    }

    #[tokio::test]
    async fn get_task_finds_by_id_prefix() {
        let state = test_state().await;
        let submitted = submit_task(&state, &json!({ "goal": "prefix lookup goal" }))
            .await
            .unwrap();
        let id = submitted["id"].as_str().unwrap().to_string();
        let resp = get_task(&state, &json!({ "task_id": id[..8] }))
            .await
            .unwrap();
        assert_eq!(resp["id"], id);
        assert_eq!(resp["goal"], "prefix lookup goal");
    }

    #[tokio::test]
    async fn get_task_reports_not_found() {
        let state = test_state().await;
        let resp = get_task(&state, &json!({ "task_id": "does-not-exist" }))
            .await
            .unwrap();
        assert_eq!(resp["error"], "task not found");
    }

    #[tokio::test]
    async fn cancel_task_deletes_a_queued_task() {
        let state = test_state().await;
        let submitted = submit_task(&state, &json!({ "goal": "cancel me" }))
            .await
            .unwrap();
        let id = submitted["id"].as_str().unwrap().to_string();
        let resp = cancel_task(&state, &json!({ "task_id": id }))
            .await
            .unwrap();
        assert_eq!(resp["deleted"], true);
        // No dispatch loop ran, so there's no live handle to signal — this
        // is the documented cross-process limitation (LEDGER.md), exercised
        // here in-process instead.
        assert_eq!(resp["cancelled"], false);
        let after = get_task(&state, &json!({ "task_id": id })).await.unwrap();
        assert_eq!(after["error"], "task not found");
    }

    #[tokio::test]
    async fn cancel_task_reports_not_found() {
        let state = test_state().await;
        let resp = cancel_task(&state, &json!({ "task_id": "nope" }))
            .await
            .unwrap();
        assert_eq!(resp["error"], "task not found");
    }

    #[tokio::test]
    async fn get_logs_unknown_task_is_an_error() {
        let state = test_state().await;
        let resp = get_logs(&state, &json!({ "task_id": "nope" }))
            .await
            .unwrap();
        assert_eq!(resp["error"], "unknown task id");
    }

    #[tokio::test]
    async fn get_logs_known_task_returns_empty_history() {
        let state = test_state().await;
        let submitted = submit_task(&state, &json!({ "goal": "log me" }))
            .await
            .unwrap();
        let id = submitted["id"].as_str().unwrap().to_string();
        let resp = get_logs(&state, &json!({ "task_id": id.clone() }))
            .await
            .unwrap();
        assert_eq!(resp["task_id"], id);
        assert!(resp["logs"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_agent_dag_unknown_task_is_an_error() {
        let state = test_state().await;
        let resp = get_agent_dag(&state, &json!({ "task_id": "nope" }))
            .await
            .unwrap();
        assert_eq!(resp["error"], "unknown task id");
    }

    #[tokio::test]
    async fn get_agent_dag_known_task_returns_empty_graph() {
        let state = test_state().await;
        let submitted = submit_task(&state, &json!({ "goal": "dag me" }))
            .await
            .unwrap();
        let id = submitted["id"].as_str().unwrap().to_string();
        let resp = get_agent_dag(&state, &json!({ "task_id": id.clone() }))
            .await
            .unwrap();
        assert_eq!(resp["task_id"], id);
        assert!(resp["nodes"].as_array().unwrap().is_empty());
        assert!(resp["edges"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_stats_reflects_queue_state() {
        let state = test_state().await;
        submit_task(&state, &json!({ "goal": "stats goal" }))
            .await
            .unwrap();
        let resp = get_stats(&state).await;
        assert_eq!(resp["queued"], 1);
        assert_eq!(resp["running"], 0);
        assert_eq!(resp["succeeded"], 0);
        assert_eq!(resp["failed"], 0);
    }

    #[tokio::test]
    async fn dispatch_routes_get_stats_to_real_json_not_a_placeholder() {
        let state = test_state().await;
        let text = dispatch(&state, "lopi_get_stats", json!({})).await.unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert!(parsed.get("running").is_some());
        assert_ne!(text, "xyzzy");
        assert_ne!(text, "");
    }

    #[tokio::test]
    async fn dispatch_rejects_unknown_tool_name() {
        let state = test_state().await;
        assert!(dispatch(&state, "not_a_real_tool", json!({}))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn tool_handler_call_round_trips_through_dispatch() {
        let handler = LopiToolHandler {
            state: test_state().await,
        };
        assert_eq!(handler.tools().len(), 7);
        let text = handler.call("lopi_list_tasks", json!({})).await.unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert!(parsed["tasks"].as_array().unwrap().is_empty());
    }
}
