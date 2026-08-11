//! `tool_defs()` — split out of `mod.rs` purely to keep that file under the
//! 500-line CI gate. No new behavior; pure move.

use super::stack_status;
use lopi_mcp::McpTool;
use serde_json::json;

/// The curated tool set: Track A's seven, MCPB-App-1's
/// `lopi_get_stack_status`, and MCPB-App-3's `lopi_list_repos` /
/// `lopi_list_branches` (the widget's stack-loop-builder view needs real
/// dropdowns, not free-text). Not extended beyond that without a concrete
/// widget need — every additional tool is context budget spent on every turn
/// a plugin user has installed.
pub(super) fn tool_defs() -> Vec<McpTool> {
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
                        "description": "Explicit worker-model override, e.g. \"claude-opus-5\". Defaults to lopi's own complexity-based selection.",
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
