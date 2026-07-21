#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Split out of `task.rs` purely to keep that file under the 500-line CI
//! file-size gate as the `budget_override` field was added; no behavioral
//! difference from being inline.

use super::*;

#[test]
fn from_template_resolves_holes_before_task_construction() {
    let vars = BTreeMap::from([
        ("repo".to_string(), "vectro".to_string()),
        ("cmd".to_string(), "cargo test".to_string()),
    ]);
    let t = Task::from_template("test {repo} until {cmd}", &vars).unwrap();
    assert_eq!(t.goal, "test vectro until cargo test");
}

#[test]
fn from_template_errors_on_missing_var_without_creating_a_task() {
    let vars = BTreeMap::new();
    assert!(Task::from_template("test {missing}", &vars).is_err());
}

#[test]
fn rubric_from_toml_str_parses_name_and_criteria() {
    let src =
        "name = \"refactor_safety\"\ncriteria = [\"No public API changes\", \"Tests still pass\"]\n";
    let rubric = Rubric::from_toml_str(src).expect("valid toml");
    assert_eq!(rubric.name, "refactor_safety");
    assert_eq!(rubric.criteria.len(), 2);
    assert_eq!(rubric.criteria[0], "No public API changes");
}

#[test]
fn rubric_from_toml_str_rejects_malformed() {
    assert!(Rubric::from_toml_str("name = ").is_err());
}

// ── Sprint Successor-1: KT-C serde round-trip ───────────────────────────────

/// KT-C — every new lineage field must be `#[serde(default)]` so a `Task`
/// JSON payload serialized before this sprint (none of `parent_task`,
/// `chain_depth`, `successor_enabled`, `successor_fixture` present) still
/// deserializes, landing on the same conservative defaults `Task::new`
/// already produces.
#[test]
fn task_deserializes_when_successor_fields_are_absent() {
    let t = Task::new("legacy payload predating successors");
    let mut json = serde_json::to_value(&t).unwrap();
    let obj = json.as_object_mut().unwrap();
    obj.remove("parent_task");
    obj.remove("chain_depth");
    obj.remove("successor_enabled");
    obj.remove("successor_fixture");
    let back: Task = serde_json::from_value(json).unwrap();
    assert!(back.parent_task.is_none());
    assert_eq!(back.chain_depth, 0);
    assert!(!back.successor_enabled);
    assert!(back.successor_fixture.is_none());
}

/// Same as above, but from a hand-authored JSON blob rather than a
/// round-tripped `Task` — proves a genuinely pre-sprint payload (not just
/// one this sprint's own serializer happened to omit fields from) parses.
#[test]
fn task_json_blob_with_none_of_the_new_fields_deserializes() {
    let json = serde_json::json!({
        "id": TaskId::new(),
        "goal": "pre-existing task",
        "constraints": [],
        "allowed_dirs": ["src/"],
        "forbidden_dirs": [],
        "priority": "Normal",
        "max_retries": 3,
        "created_at": Utc::now().to_rfc3339(),
        "source": "Cli",
    });
    let t: Task = serde_json::from_value(json).unwrap();
    assert!(t.parent_task.is_none());
    assert_eq!(t.chain_depth, 0);
    assert!(!t.successor_enabled);
    assert!(t.successor_fixture.is_none());
}

#[test]
fn task_source_selfauthored_serde_round_trip() {
    let parent = TaskId::new();
    let s = TaskSource::SelfAuthored { parent };
    let json = serde_json::to_string(&s).unwrap();
    let back: TaskSource = serde_json::from_str(&json).unwrap();
    match back {
        TaskSource::SelfAuthored { parent: p } => assert_eq!(p, parent),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn db_status_is_canonical_and_never_compound() {
    // Terminal states carry a reason/branch/paths payload, but the
    // canonical string must stay a single bare token — this is exactly
    // the invariant the old `status_label` persistence broke.
    assert_eq!(TaskStatus::Queued.db_status(), "queued");
    assert_eq!(TaskStatus::Planning.db_status(), "planning");
    assert_eq!(
        TaskStatus::AwaitingPlanApproval { attempt: 1 }.db_status(),
        "awaiting_plan_approval"
    );
    assert_eq!(TaskStatus::Implementing.db_status(), "implementing");
    assert_eq!(TaskStatus::Testing.db_status(), "testing");
    assert_eq!(TaskStatus::Scoring.db_status(), "scoring");
    assert_eq!(TaskStatus::Retrying { attempt: 2 }.db_status(), "retrying");
    assert_eq!(
        TaskStatus::Success {
            branch: "b".into(),
            pr_url: None
        }
        .db_status(),
        "success"
    );
    // A reason with an emoji must never leak — the invariant `status_label` broke.
    let failed = TaskStatus::Failed {
        reason: "boom 💥 Cancelled".into(),
    };
    assert_eq!(failed.db_status(), "failed");
    assert!(!failed.db_status().contains(' ') && failed.db_status().is_ascii());
    assert_eq!(TaskStatus::RolledBack.db_status(), "rolled_back");
    assert_eq!(
        TaskStatus::Conflict {
            paths: vec!["a.rs".into()]
        }
        .db_status(),
        "conflict"
    );
}
