//! `lopi_get_stack_status` — the MCPB-App-1 aggregating tool, plus the
//! `ui://` widget resource bound to it via `_meta.ui.resourceUri`.
//!
//! Split out of `mod.rs` to keep that file under the 500-line CI gate.
//! Neither existing tool covers what a stack-level status view needs
//! (`LEDGER.md`'s `MCP-App-1` entry, KT-D3): `lopi_get_agent_dag` is scoped
//! to one task, and `lopi_list_tasks`/`lopi_get_task` read the coarse
//! `tasks.status` column (`"running"` for the task's entire execution, no
//! stage detail). This tool joins the task roster (`load_history`) with a
//! per-task DAG read (`load_dag_nodes` → `lopi_memory::current_stage`) and
//! the branch column MCPB-App-1's KT-B1 added, so a widget has one call that
//! answers "which tasks, on which branch, at which stage."

use anyhow::Result;
use lopi_mcp::{McpResource, McpResourceContents, McpTool};
use lopi_ui::web::AppState;
use serde_json::{json, Value};

/// The `ui://` address of the stack-status widget — the one resource lopi's
/// MCP server advertises today.
pub(super) const RESOURCE_URI: &str = "ui://lopi/stack-status";

/// Self-contained HTML/JS implementing the MCP Apps lifecycle
/// (`ui/initialize` / `ui/notifications/initialized` /
/// `ui/notifications/tool-result`). Embedded at compile time so the packaged
/// binary needs no sibling file on disk to serve it.
const WIDGET_HTML: &str = include_str!("../mcp_ui/stack_status.html");

/// The `lopi_get_stack_status` tool definition, bound to [`RESOURCE_URI`]
/// via `_meta.ui.resourceUri` (MCP Apps extension, SEP-1865).
pub(super) fn tool_def() -> McpTool {
    McpTool {
        name: "lopi_get_stack_status".into(),
        description: "Get a read-only status roster of lopi tasks: goal, branch, current pipeline stage, and status. Renders as an inline dashboard on MCP Apps-supporting hosts.".into(),
        input_schema: json!({ "type": "object", "properties": {} }),
        meta: Some(json!({ "ui": { "resourceUri": RESOURCE_URI } })),
    }
}

/// The resources this server advertises via `resources/list` — just the one
/// stack-status widget.
pub(super) fn ui_resources() -> Vec<McpResource> {
    vec![McpResource {
        uri: RESOURCE_URI.into(),
        name: "lopi stack status".into(),
        description: "Read-only task roster: goal, branch, stage, status.".into(),
        mime_type: "text/html".into(),
    }]
}

/// Serve [`WIDGET_HTML`] for `resources/read`. Errors for any other URI —
/// this handler only ever advertised the one resource above.
pub(super) fn ui_resource_contents(uri: &str) -> Result<McpResourceContents> {
    if uri == RESOURCE_URI {
        Ok(McpResourceContents {
            uri: uri.to_string(),
            mime_type: "text/html".into(),
            text: WIDGET_HTML.to_string(),
        })
    } else {
        anyhow::bail!("unknown resource: {uri}")
    }
}

/// Join the task roster with each task's current DAG stage and branch.
///
/// Deliberately N+1 (one `load_dag_nodes` per roster row): the roster is
/// bounded (`load_history(100)`, same cap `lopi_list_tasks` already uses)
/// and this is a status view polled on an interval, not a hot path —
/// simplicity over a hand-rolled join query.
pub(super) async fn get_stack_status(state: &AppState) -> Value {
    let rows = state.store.load_history(100).await.unwrap_or_default();
    let mut tasks = Vec::with_capacity(rows.len());
    for t in rows {
        let stage = match state.store.load_dag_nodes(&t.id).await {
            Ok(nodes) => lopi_memory::current_stage(&nodes),
            Err(e) => {
                tracing::warn!(error = %e, task_id = %t.id, "load_dag_nodes failed");
                "unknown".to_string()
            }
        };
        tasks.push(json!({
            "id": t.id,
            "goal": t.goal,
            "status": t.status,
            "branch": t.branch,
            "stage": stage,
            "created_at": t.created_at,
            "completed_at": t.completed_at,
        }));
    }
    json!({ "tasks": tasks })
}

#[cfg(test)]
#[path = "stack_status_tests.rs"]
mod tests;
