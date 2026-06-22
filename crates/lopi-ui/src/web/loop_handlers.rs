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
use lopi_core::{AutonomyLevel, LoopConfig, SelfPromptStrategy};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// A representative failure block used to render each strategy's self-prompt
/// preview in the UI, so an engineer can see exactly what the agent would tell
/// itself before choosing a strategy.
const SAMPLE_FAILURE: &str = "Attempt 1 failed:\n  test_pass_rate: 60%\n  lint_errors: 2\n  \
     diff_lines: 48\n  errors:\n  - tests::auth::expired_token panicked";

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
            "self_prompt_strategies": self_prompt_catalog(),
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
    value["self_prompt_tag"] = json!(cfg.self_prompt.tag());
    value["self_prompt_label"] = json!(cfg.self_prompt.label());
    value["escalation_ladder"] = json!(escalation_ladder(cfg.self_prompt));
    value
}

/// The per-attempt strategy ladder for a base strategy when escalation is on:
/// attempts 1..=4, each climbing one S-rung (capped at S4). Powers the UI's
/// "what each attempt would use" visualization regardless of the current toggle.
fn escalation_ladder(base: SelfPromptStrategy) -> Vec<Value> {
    (1u8..=4)
        .map(|attempt| {
            let st = SelfPromptStrategy::escalated(base, attempt);
            json!({ "attempt": attempt, "tag": st.tag(), "label": st.label() })
        })
        .collect()
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

/// The S1–S4 self-prompting strategies as pickable options for the
/// Self-Prompting dropdown, each carrying a live preview of the self-prompt it
/// would generate from [`SAMPLE_FAILURE`].
fn self_prompt_catalog() -> Vec<Value> {
    SelfPromptStrategy::all()
        .into_iter()
        .map(|st| {
            json!({
                "value": st.tag_snake(),
                "tag": st.tag(),
                "label": st.label(),
                "description": st.description(),
                "preview": st.frame(SAMPLE_FAILURE, 1),
            })
        })
        .collect()
}

/// Body for `POST /api/loop-engineering/strategy` — the Self-Prompting picker
/// on the Loop screen writes here.
#[derive(Debug, Deserialize)]
pub(super) struct StrategyBody {
    /// Strategy tag: `direct` / `reflexion` / `self_refine` / `plan_then_act`.
    pub strategy: String,
}

/// Body for `POST /api/loop-engineering/escalation` — the adaptive-escalation
/// toggle on the Loop screen writes here.
#[derive(Debug, Deserialize)]
pub(super) struct EscalationBody {
    /// Whether the self-prompt strategy escalates one rung per failed attempt.
    pub enabled: bool,
}

/// `POST /api/loop-engineering/strategy` — set the repo's self-prompting
/// strategy and persist it to `.lopi/loop.toml` (loop-as-code, written back).
///
/// Unknown strategy tags are rejected with `422` rather than silently coerced,
/// so a typo in a client never quietly downgrades the loop.
pub(super) async fn set_strategy(
    State(s): State<AppState>,
    Json(body): Json<StrategyBody>,
) -> impl IntoResponse {
    let Some(strategy) = SelfPromptStrategy::parse(&body.strategy) else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("unknown strategy: {}", body.strategy) })),
        )
            .into_response();
    };
    persist_loop_update(
        s.repo_path.clone(),
        move |cfg| cfg.self_prompt = strategy,
        json!({
            "self_prompt": strategy.tag_snake(),
            "self_prompt_tag": strategy.tag(),
            "self_prompt_label": strategy.label(),
        }),
    )
    .await
}

/// `POST /api/loop-engineering/escalation` — toggle adaptive strategy escalation
/// and persist it to `.lopi/loop.toml`.
pub(super) async fn set_escalation(
    State(s): State<AppState>,
    Json(body): Json<EscalationBody>,
) -> impl IntoResponse {
    let enabled = body.enabled;
    persist_loop_update(
        s.repo_path.clone(),
        move |cfg| cfg.escalate_strategy = enabled,
        json!({ "escalate_strategy": enabled }),
    )
    .await
}

/// Load → mutate → persist a [`LoopConfig`] for `repo` off the async reactor,
/// returning `ok_body` on success. Shared by every loop-as-code write so the
/// filesystem/TOML round-trip and error mapping live in exactly one place.
async fn persist_loop_update<F>(
    repo: PathBuf,
    mutate: F,
    ok_body: Value,
) -> axum::response::Response
where
    F: FnOnce(&mut LoopConfig) + Send + 'static,
{
    let result = tokio::task::spawn_blocking(move || {
        let mut cfg = LoopConfig::load_from_repo(&repo)?;
        mutate(&mut cfg);
        cfg.save_to_repo(&repo)?;
        anyhow::Ok(())
    })
    .await;

    match result {
        Ok(Ok(())) => (StatusCode::OK, Json(ok_body)).into_response(),
        Ok(Err(e)) => {
            tracing::warn!("loop config update failed: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("{e:#}") })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("loop config update task panicked: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal error" })),
            )
                .into_response()
        }
    }
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
    fn self_prompt_catalog_has_four_strategies_with_previews() {
        let cat = self_prompt_catalog();
        assert_eq!(cat.len(), 4);
        assert_eq!(cat[0]["value"], "direct");
        assert_eq!(cat[1]["value"], "reflexion");
        // Direct's preview is the raw failure; richer strategies embed it.
        assert_eq!(cat[0]["preview"], SAMPLE_FAILURE);
        for entry in &cat {
            let preview = entry["preview"].as_str().unwrap_or_default();
            assert!(preview.contains("test_pass_rate"), "preview shows failure");
            assert!(!entry["description"].as_str().unwrap_or_default().is_empty());
        }
    }

    #[test]
    fn loop_config_json_carries_self_prompt_tags() {
        let cfg = LoopConfig::default();
        let v = loop_config_json(&cfg, &[]);
        assert_eq!(v["self_prompt"], "direct");
        assert_eq!(v["self_prompt_tag"], "S1");
        assert_eq!(v["self_prompt_label"], "Direct");
        assert_eq!(v["escalate_strategy"], false);
    }

    #[test]
    fn escalation_ladder_climbs_from_the_base_strategy() {
        // From Direct: S1, S2, S3, S4 across attempts 1–4.
        let ladder = escalation_ladder(SelfPromptStrategy::Direct);
        assert_eq!(ladder.len(), 4);
        assert_eq!(ladder[0]["attempt"], 1);
        assert_eq!(ladder[0]["tag"], "S1");
        assert_eq!(ladder[3]["tag"], "S4");
        // From a higher base it caps at S4 early.
        let from_s3 = escalation_ladder(SelfPromptStrategy::SelfRefine);
        assert_eq!(from_s3[0]["tag"], "S3");
        assert_eq!(from_s3[1]["tag"], "S4");
        assert_eq!(from_s3[3]["tag"], "S4");
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
