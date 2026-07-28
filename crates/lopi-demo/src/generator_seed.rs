//! Private DB-writing helpers for [`crate::generator::generate`] — split out
//! purely to keep `generator.rs` under the file-size gate. Every function
//! here takes an already-open [`MemoryStore`] and a seeded RNG and performs
//! one category of write (attempts, turn metrics, logs, DAG nodes,
//! patterns, lessons, quality trend, dead-letters). None of these read the
//! real filesystem or environment — all randomness flows from the caller's
//! `&mut StdRng`.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use lopi_core::{Attempt, Score, Task, TaskId, TurnMetrics};
use lopi_memory::{AuditInput, DemoLessonSeed, DemoPatternSeed, MemoryStore, QualityRunRecord};
use rand::rngs::StdRng;
use rand::Rng;
use uuid::Uuid;

use crate::content;
use crate::generator_content::TaskOutcome;

/// Deterministic UUID drawn from `rng` — reused everywhere a demo row needs
/// a stable-per-seed primary key.
pub(crate) fn seeded_uuid(rng: &mut StdRng) -> Uuid {
    Uuid::from_bytes(rng.gen())
}

/// Current model ids the demo's turn-metrics traffic is spread across.
const MODELS: [&str; 3] = [
    "claude-sonnet-5",
    "claude-opus-5",
    "claude-haiku-4-5-20251001",
];

/// How many attempts a task with this outcome plausibly made.
pub(crate) fn attempt_count_for(outcome: TaskOutcome, max_retries: u8, rng: &mut StdRng) -> u8 {
    match outcome {
        TaskOutcome::Queued => 0,
        TaskOutcome::Running | TaskOutcome::RolledBack => rng.gen_range(1..=2),
        TaskOutcome::Success => {
            if rng.gen_bool(0.4) {
                2
            } else {
                1
            }
        }
        TaskOutcome::Failed => max_retries.max(1),
        TaskOutcome::Conflict => 1,
    }
}

/// Persist `count` attempts for `task`, the last one scored to match
/// `outcome`; earlier ones (a retry cycle) score as failed attempts.
///
/// # Errors
/// Returns `Err` if any attempt insert fails.
pub(crate) async fn write_attempts(
    store: &MemoryStore,
    rng: &mut StdRng,
    task: &Task,
    outcome: TaskOutcome,
    count: u8,
) -> Result<()> {
    let mut when = task.created_at;
    for attempt_num in 1..=count {
        when += Duration::minutes(rng.gen_range(2i64..45));
        let branch = format!("lopi/{}-attempt-{attempt_num}", task.id);
        let mut attempt = Attempt::new(task.id, attempt_num, branch);
        attempt.created_at = when;
        let is_last = attempt_num == count;
        let (pass_rate, lint_errors, diff_lines, outcome_str): (f32, u32, u32, &str) = if is_last {
            match outcome {
                TaskOutcome::Success | TaskOutcome::Conflict => {
                    (1.0, 0, rng.gen_range(20..600), "success")
                }
                TaskOutcome::Failed => (
                    rng.gen_range(0.4..0.9),
                    rng.gen_range(1..6),
                    rng.gen_range(50..900),
                    "failed",
                ),
                TaskOutcome::RolledBack => (
                    rng.gen_range(0.5..0.95),
                    0,
                    rng.gen_range(80..500),
                    "failed",
                ),
                TaskOutcome::Running | TaskOutcome::Queued => (0.0, 0, 0, "pending"),
            }
        } else {
            (
                rng.gen_range(0.3..0.8),
                rng.gen_range(1..5),
                rng.gen_range(40..700),
                "failed",
            )
        };
        attempt.score = Some(Score::new(pass_rate, lint_errors, diff_lines));
        attempt.outcome = outcome_str.to_string();
        store.save_attempt(&attempt).await?;
    }
    Ok(())
}

/// Persist 2-5 `turn_metrics` rows for `task_id`. `today_bias` stamps a
/// couple of rows with `Utc::now()` (rather than backdated) so
/// `/api/stats`' daily token total shows nonzero activity — see the sprint
/// spec's requirement that at least some running/success tasks show up in
/// "today".
///
/// # Errors
/// Returns `Err` if any turn-metrics insert fails.
pub(crate) async fn write_turn_metrics(
    store: &MemoryStore,
    rng: &mut StdRng,
    task_id: TaskId,
    session_id: Uuid,
    today_bias: bool,
) -> Result<()> {
    let n: u8 = rng.gen_range(2..=5);
    for i in 0..n {
        let model = MODELS[rng.gen_range(0..MODELS.len())];
        let input_tokens: u32 = rng.gen_range(400..18_000);
        let output_tokens: u32 = rng.gen_range(100..6_000);
        let cache_read_input_tokens: u32 = rng.gen_range(0..(input_tokens / 2 + 1));
        let cache_write_input_tokens: u32 = rng.gen_range(0..2_000);
        let ttft_ms: u64 = rng.gen_range(200..3_000);
        let turn_latency_ms: u64 = ttft_ms + rng.gen_range(500..12_000);
        let tool_execution_ms: u64 = rng.gen_range(0..(turn_latency_ms / 2 + 1));
        let context_tokens = input_tokens + output_tokens;
        let context_pressure = (context_tokens as f32 / 200_000.0).min(1.0);
        let estimated_cost_usd =
            f64::from(input_tokens) * 0.000_003 + f64::from(output_tokens) * 0.000_015;
        let timestamp = if today_bias && i < 2 {
            Utc::now()
        } else {
            Utc::now() - Duration::hours(rng.gen_range(1i64..240))
        };
        let m = TurnMetrics {
            turn_id: seeded_uuid(rng),
            task_id,
            session_id,
            model: model.to_string(),
            attempt_number: 1,
            input_tokens,
            output_tokens,
            cache_read_input_tokens,
            cache_write_input_tokens,
            ttft_ms,
            turn_latency_ms,
            tool_execution_ms,
            context_tokens,
            context_pressure,
            evictions_this_turn: rng.gen_range(0..3),
            tool_calls: rng.gen_range(0..8),
            tools_parallel: rng.gen_bool(0.3),
            estimated_cost_usd,
            timestamp,
        };
        store.save_turn_metrics(&m).await?;
    }
    Ok(())
}

/// Persist 3-8 log lines for `task_id`, plus one `error` line when
/// `outcome` is `Failed`.
///
/// # Errors
/// Returns `Err` if any log insert fails.
pub(crate) async fn write_task_logs(
    store: &MemoryStore,
    rng: &mut StdRng,
    task_id: &str,
    base_ts: DateTime<Utc>,
    outcome: TaskOutcome,
) -> Result<()> {
    let n: u8 = rng.gen_range(3..=8);
    let stages = ["plan", "implement", "test", "score"];
    for i in 0..n {
        let template =
            content::LOG_LINE_TEMPLATES[rng.gen_range(0..content::LOG_LINE_TEMPLATES.len())];
        let stage = stages[rng.gen_range(0..stages.len())];
        let line = template.replace("{stage}", stage);
        let level = if rng.gen_bool(0.15) { "warn" } else { "info" };
        let ts = base_ts + Duration::seconds(i64::from(i) * rng.gen_range(10i64..120));
        store.record_task_log(task_id, ts, level, &line).await?;
    }
    if outcome == TaskOutcome::Failed {
        let ts = base_ts + Duration::minutes(rng.gen_range(5i64..90));
        store
            .record_task_log(
                task_id,
                ts,
                "error",
                "test suite failed: assertions did not pass after the final retry",
            )
            .await?;
    }
    Ok(())
}

/// Write the 4-stage `plan -> implement -> test -> score` DAG pipeline for
/// one task — `done` through every stage for a finished task, or `done`
/// through `plan` with `implement` `running` and the rest `pending` for one
/// still in flight.
///
/// # Errors
/// Returns `Err` if any DAG node upsert fails.
pub(crate) async fn write_dag(
    store: &MemoryStore,
    task_id: &str,
    still_running: bool,
) -> Result<()> {
    let stages = ["plan", "implement", "test", "score"];
    let running_at = 1usize; // "implement" — a plausible mid-pipeline point.
    for (i, stage) in stages.iter().enumerate() {
        let deps = if i == 0 {
            "[]".to_string()
        } else {
            format!("[{:?}]", stages[i - 1])
        };
        let status = if still_running {
            match i.cmp(&running_at) {
                std::cmp::Ordering::Less => "done",
                std::cmp::Ordering::Equal => "running",
                std::cmp::Ordering::Greater => "pending",
            }
        } else {
            "done"
        };
        store
            .upsert_dag_node(task_id, stage, status, &deps, None, None)
            .await?;
    }
    Ok(())
}

/// Seed every `content::PATTERNS` entry as a `patterns` row.
///
/// # Errors
/// Returns `Err` if any pattern insert fails.
pub(crate) async fn write_patterns(store: &MemoryStore, rng: &mut StdRng) -> Result<()> {
    for (i, (keywords, constraint)) in content::PATTERNS.iter().enumerate() {
        let toolchain = match rng.gen_range(0..4) {
            0 => Some("rust".to_string()),
            1 => Some("typescript".to_string()),
            2 => Some("python".to_string()),
            _ => None,
        };
        let seed = DemoPatternSeed {
            id: seeded_uuid(rng).to_string(),
            goal_keywords: (*keywords).to_string(),
            successful_constraints: Some(format!("[{constraint:?}]")),
            avg_attempts: rng.gen_range(1.0..3.0),
            success_rate: rng.gen_range(0.55..0.95),
            last_seen: Utc::now() - Duration::hours(rng.gen_range(1i64..72)),
            derived_from_postmortem: i.is_multiple_of(2),
            toolchain,
            occurrence_count: rng.gen_range(3..40),
        };
        store.seed_demo_pattern(&seed).await?;
    }
    Ok(())
}

/// Seed every `content::LESSONS` entry as a `lessons` row.
///
/// # Errors
/// Returns `Err` if any lesson insert fails.
pub(crate) async fn write_lessons(store: &MemoryStore, rng: &mut StdRng) -> Result<()> {
    for (category, text) in content::LESSONS {
        let repo = &content::REPOS[rng.gen_range(0..content::REPOS.len())];
        let seed = DemoLessonSeed {
            id: seeded_uuid(rng).to_string(),
            repo_path: repo.path.to_string(),
            category: (*category).to_string(),
            content: (*text).to_string(),
            task_id: None,
            created_at: Utc::now() - Duration::hours(rng.gen_range(1i64..200)),
        };
        store.seed_demo_lesson(&seed).await?;
    }
    Ok(())
}

/// Write 6 quality-check runs for `repo_path` whose score sequence traces a
/// visible, non-monotonic arc (`~0.62 -> 0.68 -> 0.71 -> 0.66 -> 0.74 -> 0.79`).
/// `save_quality_run` always stamps `run_at` as "now", so all 6 land at
/// nearly the same instant — fine for a demo trend chart, which only needs
/// a visible arc, not real spacing.
///
/// # Errors
/// Returns `Err` if any quality-run insert fails.
pub(crate) async fn write_quality_trend(store: &MemoryStore, repo_path: &str) -> Result<()> {
    const PASSING: [usize; 6] = [62, 68, 71, 66, 74, 79];
    for passing in PASSING {
        let gaps = 3;
        let failing = 100 - passing - gaps;
        store
            .save_quality_run(QualityRunRecord {
                repo_path: repo_path.to_string(),
                spec_items: 100,
                passing,
                failing,
                gaps,
            })
            .await?;
    }
    Ok(())
}

/// Record one `task.dead_letter` audit row for `task_id`, quoting `blocker`
/// under a `"blocker"` JSON key.
///
/// # Errors
/// Returns `Err` if the audit insert fails.
pub(crate) async fn write_dead_letter(
    store: &MemoryStore,
    task_id: &str,
    blocker: &str,
) -> Result<()> {
    let payload = format!(r#"{{"blocker":{blocker:?}}}"#);
    store
        .record_audit(
            &AuditInput::new("task.dead_letter")
                .subject("task", task_id)
                .actor("pool")
                .payload_json(payload),
        )
        .await?;
    Ok(())
}
