use anyhow::Result;
use lopi_core::{Priority, RepoProfile, ScheduleEntry, Task, TaskSource};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, warn};

use crate::pool::AgentPool;

/// Boot a `JobScheduler` from a list of `ScheduleEntry` values.
/// Each entry fires as a tokio cron job that submits a `Task` to the pool.
pub async fn boot(entries: Vec<ScheduleEntry>, pool: AgentPool) -> Result<JobScheduler> {
    let sched = JobScheduler::new().await?;

    for entry in entries {
        let pool = pool.clone();
        let entry = entry.clone();
        // Extract strings needed for error reporting before the closure moves `entry`.
        let cron_str = entry.cron.clone();
        let entry_name = entry.name.clone();

        let job = match Job::new_async(cron_str.as_str(), move |_uuid, _lock| {
            let pool = pool.clone();
            let entry = entry.clone();
            Box::pin(async move {
                info!(schedule = %entry.name, "firing scheduled task: {}", entry.goal);

                let mut task = Task::new(entry.goal.clone());
                task.source = TaskSource::Api; // Scheduled tasks come from the scheduler.
                task.priority = match entry.priority.as_str() {
                    "low"      => Priority::Low,
                    "high"     => Priority::High,
                    "critical" => Priority::Critical,
                    _          => Priority::Normal,
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
}
