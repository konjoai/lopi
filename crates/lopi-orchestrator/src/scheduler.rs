use anyhow::Result;
use lopi_core::{Priority, RepoProfile, ScheduleEntry, Task, TaskSource};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, warn};

use crate::pool::AgentPool;

/// Boot a `JobScheduler` from a list of `ScheduleEntry` values.
/// Each entry fires as a tokio cron job that submits a `Task` to the pool.
///
/// # Errors
///
/// Returns an error if the scheduler cannot be created or started.
pub async fn boot(entries: Vec<ScheduleEntry>, pool: AgentPool) -> Result<JobScheduler> {
    let sched = JobScheduler::new().await?;

    for entry in entries {
        let pool = pool.clone();
        let entry = entry.clone();
        // Extract strings needed for error reporting before the closure moves `entry`.
        let cron_str = entry.cron.clone();
        let entry_name = entry.name.clone();

        // `tokio-cron-scheduler` (via the `cron` crate) requires a 6-field
        // expression with a leading seconds field. lopi's user-facing
        // convention — matching `next_run_times` — is 5-field with seconds
        // pinned to 0, so prepend it here (mirrors `schedule_manager::register`
        // and `chain_schedule_manager`'s handling of the same entry format).
        let six_field = format!("0 {cron_str}");

        let job = match Job::new_async(six_field.as_str(), move |_uuid, _lock| {
            let pool = pool.clone();
            let entry = entry.clone();
            Box::pin(async move {
                info!(schedule = %entry.name, "firing scheduled task: {}", entry.goal);

                let mut task = Task::new(entry.goal.clone());
                task.source = TaskSource::Api; // Scheduled tasks come from the scheduler.
                task.priority = match entry.priority.as_str() {
                    "low" => Priority::Low,
                    "high" => Priority::High,
                    "critical" => Priority::Critical,
                    _ => Priority::Normal,
                };
                if !entry.allowed_dirs.is_empty() {
                    task.allowed_dirs = entry.allowed_dirs.clone();
                }
                if !entry.forbidden_dirs.is_empty() {
                    task.forbidden_dirs = entry.forbidden_dirs.clone();
                }

                // Apply per-repo profile if present.
                let profile = RepoProfile::load_from_repo(&entry.repo);
                profile.apply(&mut task);

                // Loop engineering — carry the schedule's L1–L4 trust level onto
                // the task so the runner enforces it (report-only / draft PR /
                // verified PR / auto-merge). Without this the trust dropdown
                // would be cosmetic.
                task.autonomy_level = entry.autonomy_level;

                // Report on Finish — carry the schedule's declared channel
                // onto the task so the L1 report-only hook (or, once
                // implemented, other levels) can route a summary there.
                task.report = entry.report.clone();

                pool.submit(task).await;
            })
        }) {
            Ok(j) => j,
            Err(e) => {
                warn!(schedule = %entry_name, "invalid cron expression '{}': {e}", cron_str);
                continue;
            }
        };

        sched.add(job).await?;
        info!(schedule = %entry_name, cron = %cron_str, "registered schedule");
    }

    sched.start().await?;
    Ok(sched)
}

/// Format the next N fire times for a cron expression (uses chrono + cron crate logic).
/// Returns empty vec if expression is invalid.
#[must_use]
pub fn next_run_times(cron_expr: &str, count: usize) -> Vec<chrono::DateTime<chrono::Utc>> {
    // tokio-cron-scheduler uses the `cron` crate internally.
    // We parse directly here to show next-run times in `lopi schedules list`.
    use std::str::FromStr;
    let Ok(schedule) = cron::Schedule::from_str(&format!("0 {cron_expr}")) else {
        return vec![];
    };
    schedule.upcoming(chrono::Utc).take(count).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn next_run_times_valid_expr() {
        let times = next_run_times("0 2 * * *", 3);
        assert_eq!(times.len(), 3);
        // All times should be in the future.
        let now = chrono::Utc::now();
        for t in &times {
            assert!(t > &now, "next run time should be in the future");
        }
    }

    #[test]
    fn next_run_times_invalid_expr() {
        let times = next_run_times("not a cron", 3);
        assert!(times.is_empty());
    }

    #[test]
    fn next_run_times_returns_correct_count() {
        let times = next_run_times("0 2 * * *", 5);
        assert_eq!(times.len(), 5);
    }

    #[test]
    fn next_run_times_count_zero_returns_empty() {
        let times = next_run_times("0 2 * * *", 0);
        assert!(times.is_empty());
    }

    #[test]
    fn next_run_times_times_are_ordered() {
        let times = next_run_times("0 2 * * *", 4);
        assert_eq!(times.len(), 4);
        for i in 1..times.len() {
            assert!(
                times[i] > times[i - 1],
                "cron times should be strictly increasing"
            );
        }
    }

    #[test]
    fn next_run_times_weekly_expr() {
        let times = next_run_times("0 9 * * MON", 2);
        assert_eq!(times.len(), 2);
        let now = chrono::Utc::now();
        for t in &times {
            assert!(t > &now);
        }
    }

    #[test]
    fn next_run_times_every_minute_expr() {
        // "* * * * *" = every minute
        let times = next_run_times("* * * * *", 3);
        assert_eq!(times.len(), 3);
    }

    #[test]
    fn next_run_times_empty_string_returns_empty() {
        let times = next_run_times("", 3);
        assert!(times.is_empty());
    }

    #[test]
    fn next_run_times_partial_cron_returns_empty() {
        // Only 3 fields — invalid 5-field cron
        let times = next_run_times("0 2 *", 3);
        assert!(times.is_empty());
    }

    #[test]
    fn next_run_times_all_fields_wildcard() {
        // "* * * * *" fires every minute
        let times = next_run_times("* * * * *", 10);
        assert_eq!(times.len(), 10);
        // All times should be within the next hour (every minute)
        let now = chrono::Utc::now();
        let one_hour = chrono::Duration::hours(1);
        for t in &times {
            assert!(t > &now);
            assert!(t < &(now + one_hour));
        }
    }

    /// Build a `ScheduleEntry` with a fixed nonexistent repo and no dir
    /// overrides — shared by every `boot()` test in this module.
    fn make_entry(name: &str, goal: &str, cron: &str, priority: &str) -> lopi_core::ScheduleEntry {
        lopi_core::ScheduleEntry {
            name: name.to_string(),
            repo: std::path::PathBuf::from("/tmp/nonexistent"),
            goal: goal.to_string(),
            cron: cron.to_string(),
            priority: priority.to_string(),
            allowed_dirs: vec![],
            forbidden_dirs: vec![],
            autonomy_level: Default::default(),
            report: None,
        }
    }

    #[tokio::test]
    async fn boot_with_empty_entries_returns_scheduler() {
        use crate::pool::AgentPool;
        use lopi_core::{AgentEvent, EventBus};
        use std::path::PathBuf;

        let queue = crate::queue::TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let pool = AgentPool::new(1, PathBuf::from("."), queue, bus);

        let result = boot(vec![], pool).await;
        assert!(result.is_ok(), "boot with empty entries should succeed");
        let mut sched = result.unwrap();
        sched.shutdown().await.unwrap();
    }

    /// Regression pin for the cron seconds-field mismatch: `Job::new_async`
    /// (what `boot()` calls) requires a 6-field expression with a leading
    /// seconds field — a bare 5-field cron string (the documented
    /// `ScheduleEntry.cron` format, e.g. `"0 2 * * *"`) fails to parse on
    /// its own. `boot()` used to pass the raw 5-field string straight
    /// through, so every schedule silently failed and was skipped (only a
    /// `warn!` marked it, while `boot()` itself still returned `Ok`).
    /// Pins that prepending the seconds field — what `boot()` now does —
    /// is what makes it parse.
    #[test]
    fn five_field_cron_alone_fails_six_field_with_prepended_seconds_succeeds() {
        let five_field = "0 2 * * *";
        assert!(
            Job::new_async(five_field, |_uuid, _lock| Box::pin(async {})).is_err(),
            "a bare 5-field cron string must fail Job::new_async"
        );

        let six_field = format!("0 {five_field}");
        assert!(
            Job::new_async(six_field.as_str(), |_uuid, _lock| Box::pin(async {})).is_ok(),
            "prepending the seconds field must make it parse"
        );
    }

    #[tokio::test]
    async fn boot_with_valid_entry_registers_job() {
        use crate::pool::AgentPool;
        use lopi_core::{AgentEvent, EventBus};
        use std::path::PathBuf;

        let queue = crate::queue::TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let pool = AgentPool::new(1, PathBuf::from("."), queue, bus);

        let entry = make_entry("test-schedule", "run tests", "0 2 * * *", "normal");

        let result = boot(vec![entry], pool).await;
        assert!(result.is_ok(), "boot with valid entry should succeed");
        let mut sched = result.unwrap();
        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn boot_skips_invalid_cron_entry() {
        use crate::pool::AgentPool;
        use lopi_core::{AgentEvent, EventBus};
        use std::path::PathBuf;

        let queue = crate::queue::TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let pool = AgentPool::new(1, PathBuf::from("."), queue, bus);

        let bad_entry = make_entry(
            "bad-cron",
            "do something",
            "not a valid cron expression",
            "normal",
        );

        // Invalid cron entry should be skipped, not cause boot to fail
        let result = boot(vec![bad_entry], pool).await;
        assert!(result.is_ok(), "boot should succeed even with invalid cron");
        let mut sched = result.unwrap();
        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn boot_with_allowed_dirs_entry() {
        use crate::pool::AgentPool;
        use lopi_core::{AgentEvent, EventBus, ScheduleEntry};
        use std::path::PathBuf;

        let queue = crate::queue::TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let pool = AgentPool::new(1, PathBuf::from("."), queue, bus);

        let entry = ScheduleEntry {
            name: "dir-restricted".to_string(),
            repo: PathBuf::from("/tmp/nonexistent"),
            goal: "fix linting".to_string(),
            cron: "0 3 * * *".to_string(),
            priority: "high".to_string(),
            allowed_dirs: vec!["src/".to_string()],
            forbidden_dirs: vec!["vendor/".to_string()],
            autonomy_level: Default::default(),
            report: None,
        };

        let result = boot(vec![entry], pool).await;
        assert!(result.is_ok());
        let mut sched = result.unwrap();
        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn boot_with_multiple_valid_entries() {
        use crate::pool::AgentPool;
        use lopi_core::{AgentEvent, EventBus};
        use std::path::PathBuf;

        let queue = crate::queue::TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let pool = AgentPool::new(2, PathBuf::from("."), queue, bus);

        let entries = vec![
            make_entry("entry-1", "task one", "0 1 * * *", "low"),
            make_entry("entry-2", "task two", "0 2 * * *", "critical"),
        ];

        let result = boot(entries, pool).await;
        assert!(result.is_ok());
        let mut sched = result.unwrap();
        sched.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn boot_with_mixed_valid_invalid_entries() {
        use crate::pool::AgentPool;
        use lopi_core::{AgentEvent, EventBus};
        use std::path::PathBuf;

        let queue = crate::queue::TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(2);
        let pool = AgentPool::new(2, PathBuf::from("."), queue, bus);

        let entries = vec![
            make_entry("valid", "run valid task", "0 4 * * *", "normal"),
            make_entry("invalid", "run invalid task", "invalid cron", "normal"),
        ];

        // Should succeed — invalid entry is skipped
        let result = boot(entries, pool).await;
        assert!(result.is_ok());
        let mut sched = result.unwrap();
        sched.shutdown().await.unwrap();
    }
}
