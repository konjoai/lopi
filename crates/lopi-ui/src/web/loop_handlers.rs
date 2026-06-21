//! Loop Engineering aggregation endpoint — the backend for the Loop screen.
//!
//! `GET /api/loop-engineering` composes a single read-only snapshot of every
//! loop-engineering lever for the primary repo: the effective `LoopConfig`
//! (with validation status), the discovered skills and rules, the live
//! schedules (each carrying its L1–L4 trust level), and the Konjo quality-gate
//! thresholds. Both the web and macOS Loop screens render this one payload.
//!
//! Route (behind the shared Bearer-auth + rate-limit middleware):
//! - `GET /api/loop-engineering` — the full loop snapshot.

use super::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_core::{AutonomyLevel, LoopConfig};
use serde_json::{json, Value};
use std::path::Path;

/// `GET /api/loop-engineering` — the aggregated loop snapshot for the primary repo.
pub(super) async fn get_loop(State(s): State<AppState>) -> impl IntoResponse {
    let repo = s.repo_path.clone();
    // Filesystem + TOML reads run off the async reactor.
    let (config_json, skills, rules) = tokio::task::spawn_blocking(move || scan_repo(&repo))
        .await
        .unwrap_or_else(|_| {
            (
                loop_config_json(&LoopConfig::default(), &[]),
                vec![],
                vec![],
            )
        });

    let schedules = s
        .store
        .list_schedules()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(schedule_summary)
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        Json(json!({
            "repo": s.repo_path.display().to_string(),
            "config": config_json,
            "autonomy_levels": autonomy_catalog(),
            "skills": skills,
            "rules": rules,
            "schedules": schedules,
            "gates": gate_catalog(),
        })),
    )
        .into_response()
}

/// Load the loop config and discover skills + rules for a repo (blocking).
fn scan_repo(repo: &Path) -> (Value, Vec<Value>, Vec<Value>) {
    let cfg = LoopConfig::load_from_repo(repo).unwrap_or_default();
    let issues = cfg.validate(repo);
    let config_json = loop_config_json(&cfg, &issues);
    let skills = discover_skills(&repo.join(".claude/skills"));
    let rules = discover_rules(&repo.join(".claude/rules"));
    (config_json, skills, rules)
}

/// Serialize a [`LoopConfig`] with a `present`/`valid`/`issues` envelope.
fn loop_config_json(cfg: &LoopConfig, issues: &[String]) -> Value {
    let mut value = serde_json::to_value(cfg).unwrap_or_else(|_| json!({}));
    value["valid"] = json!(issues.is_empty());
    value["issues"] = json!(issues);
    value["autonomy_label"] = json!(cfg.autonomy_level.label());
    value["autonomy_tag"] = json!(cfg.autonomy_level.tag());
    value
}

/// Each `.claude/skills/<name>/SKILL.md` → `{name, description}`.
fn discover_skills(dir: &Path) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let (name, description) = parse_frontmatter(&skill_md, &dir_name);
        out.push(json!({ "name": name, "description": description }));
    }
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    out
}

/// Each `.claude/rules/<name>.md` → `{name}`.
fn discover_rules(dir: &Path) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            if let Some(stem) = path.file_stem() {
                out.push(json!({ "name": stem.to_string_lossy() }));
            }
        }
    }
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    out
}

/// Pull `name:` and `description:` from a SKILL.md YAML frontmatter block,
/// falling back to the directory name and an empty description.
fn parse_frontmatter(path: &Path, fallback_name: &str) -> (String, String) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (fallback_name.to_string(), String::new());
    };
    let mut name = fallback_name.to_string();
    let mut description = String::new();
    for line in text.lines().take(20) {
        if let Some(rest) = line.strip_prefix("name:") {
            name = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("description:") {
            description = rest.trim().trim_matches('"').to_string();
        }
    }
    (name, description)
}

/// Project a stored schedule row to the loop-screen summary shape.
fn schedule_summary(row: lopi_memory::ScheduleRow) -> Value {
    let level = AutonomyLevel::parse(&row.autonomy_level).unwrap_or_default();
    json!({
        "id": row.id,
        "name": row.name,
        "goal": row.goal,
        "cron": row.cron,
        "enabled": row.enabled,
        "autonomy_level": row.autonomy_level,
        "autonomy_tag": level.tag(),
        "autonomy_label": level.label(),
    })
}

/// The L1–L4 ladder as pickable options for the Trust-Level dropdown.
fn autonomy_catalog() -> Vec<Value> {
    AutonomyLevel::all()
        .into_iter()
        .map(|l| {
            json!({
                "value": l.tag_snake(),
                "tag": l.tag(),
                "label": l.label(),
                "opens_pr": l.opens_pr(),
                "requires_verifier": l.requires_verifier(),
                "allows_auto_merge": l.allows_auto_merge(),
            })
        })
        .collect()
}

/// The Konjo quality walls surfaced as the loop's guardrail gates.
fn gate_catalog() -> Vec<Value> {
    vec![
        json!({ "wall": "Wall 1", "name": "Pre-commit", "checks": "cargo check · clippy · unwrap scan · dead-code · file-size · DRY · placeholder scan" }),
        json!({ "wall": "Wall 2", "name": "CI gate", "checks": "coverage ≥ 80% · mutation ≤ 10% · complexity ≤ 15 · zero undocumented APIs" }),
        json!({ "wall": "Wall 3", "name": "Adversarial review", "checks": "Opus reviews every PR against 10 mandatory questions" }),
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn autonomy_catalog_has_four_levels() {
        let cat = autonomy_catalog();
        assert_eq!(cat.len(), 4);
        assert_eq!(cat[0]["value"], "report_only");
        assert_eq!(cat[3]["allows_auto_merge"], true);
    }

    #[test]
    fn gate_catalog_lists_three_walls() {
        assert_eq!(gate_catalog().len(), 3);
    }

    #[test]
    fn loop_config_json_carries_validation() {
        let cfg = LoopConfig::default();
        let v = loop_config_json(&cfg, &[]);
        assert_eq!(v["valid"], true);
        assert_eq!(v["autonomy_tag"], "L2");
    }

    #[test]
    fn discover_skills_and_rules_handle_missing_dirs() {
        let nope = Path::new("/tmp/lopi_no_such_dir_xyz");
        assert!(discover_skills(nope).is_empty());
        assert!(discover_rules(nope).is_empty());
    }

    #[test]
    fn parse_frontmatter_falls_back_to_dir_name() {
        let (name, desc) = parse_frontmatter(Path::new("/tmp/nope.md"), "fallback");
        assert_eq!(name, "fallback");
        assert!(desc.is_empty());
    }
}
