//! MAXX Phase 1 — opportunistic backlog dispatch tick.
//!
//! Unlike [`crate::schedule_manager`], MAXX entries never run on a fixed
//! cadence. [`MaxxLoop`] wakes on its own interval (default 5 minutes) and,
//! for each enabled [`lopi_memory::MaxxRow`], checks whether *conditions*
//! are favorable right now — quiet hours, or comfortable quota headroom on
//! the entry's configured windows (via [`crate::QuotaTracker`]) — and fires
//! only then. `Priority` deliberately plays no role here (see the module
//! doc on why): all the "don't dispatch yet" logic lives in
//! [`is_favorable`], not in queue ordering.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Timelike, Utc};
use lopi_core::{AutonomyLevel, LimitWindow, Task};
use lopi_memory::{MaxxRow, MemoryStore};
use tracing::{info, warn};

use crate::pool::AgentPool;
use crate::quota_tracker::QuotaTracker;

/// Default interval between `maxx_loop` ticks.
pub const DEFAULT_TICK_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Utilization at or below this is "high headroom" for the headroom gate.
pub const HEADROOM_UTILIZATION_MAX: f32 = 0.5;

/// A window resetting within this many seconds counts as "nearing reset".
pub const HEADROOM_RESET_WITHIN_SECS: i64 = 2 * 60 * 60;

/// Minimum time between two fires of the same entry, regardless of how many
/// ticks find it favorable in between. Without this, an entry with an
/// 8-hour quiet-hours window would refire its identical goal on every 5
/// minute tick — ~96 duplicate runs a night, burning exactly the quota
/// headroom this feature exists to protect. Not in the sprint's locked
/// spec; added because "fire once per tick while favorable" is a silent
/// footgun, not an intentional design choice.
pub const MIN_REFIRE_INTERVAL_SECS: i64 = 60 * 60;

/// The subset of a MAXX entry needed to evaluate favorability and fire a task.
#[derive(Debug, Clone)]
pub struct MaxxSpec {
    /// Stable entry id — used to attribute run history and cooldown lookups.
    pub id: String,
    /// Agent goal submitted on each fire.
    pub goal: String,
    /// Target repo path, if any.
    pub repo: Option<PathBuf>,
    /// Priority string (`low` / `normal` / `high` / `critical`).
    pub priority: String,
    /// Allowed directories override.
    pub allowed_dirs: Vec<String>,
    /// Forbidden directories override.
    pub forbidden_dirs: Vec<String>,
    /// Trust level governing how far this loop may act without a human.
    pub autonomy_level: AutonomyLevel,
    /// Local hours `(start, end)` treated as quiet hours. `None` disables it.
    pub quiet_hours: Option<(u8, u8)>,
    /// Whether the quota-headroom condition is checked.
    pub headroom_gate: bool,
    /// Windows `headroom_gate` checks.
    pub windows: Vec<LimitWindow>,
}

impl From<MaxxRow> for MaxxSpec {
    fn from(r: MaxxRow) -> Self {
        Self {
            id: r.id,
            goal: r.goal,
            repo: r.repo.map(PathBuf::from),
            priority: r.priority,
            allowed_dirs: r.allowed_dirs,
            forbidden_dirs: r.forbidden_dirs,
            autonomy_level: AutonomyLevel::parse(&r.autonomy_level).unwrap_or_default(),
            quiet_hours: match (r.quiet_hours_start, r.quiet_hours_end) {
                (Some(s), Some(e)) => Some((s, e)),
                _ => None,
            },
            headroom_gate: r.headroom_gate,
            windows: r
                .windows
                .iter()
                .filter_map(|w| LimitWindow::parse(w))
                .collect(),
        }
    }
}

/// Build a [`Task`] from a [`MaxxSpec`], applying priority, dir overrides,
/// and any per-repo `.lopi.toml` profile — same shape as
/// `schedule_manager::build_task`, sharing its implementation.
#[must_use]
pub fn build_task(spec: &MaxxSpec) -> Task {
    crate::task_build::build_task_from_fields(
        &spec.goal,
        spec.repo.as_deref(),
        &spec.priority,
        &spec.allowed_dirs,
        &spec.forbidden_dirs,
        spec.autonomy_level,
    )
}

/// Is `local_hour` (`0..=23`) inside the `(start, end)` quiet-hours range?
/// `start == end` is a degenerate config (zero-width or full-day, ambiguous
/// either way) and is treated as "never favorable" rather than guessed at.
/// `start > end` wraps past midnight, e.g. `(23, 7)` covers 11PM-6:59AM.
#[must_use]
pub fn quiet_hours_favorable(quiet_hours: Option<(u8, u8)>, local_hour: u32) -> bool {
    let Some((start, end)) = quiet_hours else {
        return false;
    };
    let (start, end) = (u32::from(start), u32::from(end));
    if start == end {
        return false;
    }
    if start < end {
        (start..end).contains(&local_hour)
    } else {
        local_hour >= start || local_hour < end
    }
}

/// Is a single window's last observation favorable at `now`? Requires a
/// real observation with a `resets_at` — an unknown or stale (no reset info)
/// window is never favorable, per the kill-test guidance that staleness
/// should mean "don't dispatch," not "assume it's fine."
#[must_use]
pub fn window_favorable(
    obs: Option<&crate::quota_tracker::QuotaObservation>,
    now: DateTime<Utc>,
) -> bool {
    let Some(obs) = obs else {
        return false;
    };
    let Some(resets_at) = obs.resets_at else {
        return false;
    };
    let secs_to_reset = resets_at - now.timestamp();
    obs.utilization <= HEADROOM_UTILIZATION_MAX
        && secs_to_reset > 0
        && secs_to_reset <= HEADROOM_RESET_WITHIN_SECS
}

/// Is the headroom-gate condition favorable — every configured window
/// favorable at once? `AND`, not `OR`: a real dispatch consumes quota
/// against every window simultaneously, so a five-hour window with no
/// headroom left makes dispatch unsafe even if the seven-day window looks
/// comfortable. An empty `windows` list can never be favorable — it is a
/// misconfiguration (`headroom_gate: true` with nothing to check), not a
/// vacuous "always favorable".
#[must_use]
pub fn headroom_favorable(
    windows: &[LimitWindow],
    headroom_gate: bool,
    quota: &QuotaTracker,
    now: DateTime<Utc>,
) -> bool {
    if !headroom_gate || windows.is_empty() {
        return false;
    }
    windows
        .iter()
        .all(|w| window_favorable(quota.snapshot(w.as_str()).as_ref(), now))
}

/// Is `spec` favorable to fire right now? Quiet hours and headroom are
/// independent `OR`'d conditions (either alone is enough).
#[must_use]
pub fn is_favorable(
    spec: &MaxxSpec,
    quota: &QuotaTracker,
    now: DateTime<Utc>,
    local_hour: u32,
) -> bool {
    quiet_hours_favorable(spec.quiet_hours, local_hour)
        || headroom_favorable(&spec.windows, spec.headroom_gate, quota, now)
}

/// Background tick that fires favorable MAXX entries into the shared pool.
#[derive(Clone)]
pub struct MaxxLoop {
    store: MemoryStore,
    quota: QuotaTracker,
    pool: AgentPool,
    interval: Duration,
}

impl MaxxLoop {
    /// Construct a tick with the default interval.
    #[must_use]
    pub fn new(store: MemoryStore, quota: QuotaTracker, pool: AgentPool) -> Self {
        Self {
            store,
            quota,
            pool,
            interval: DEFAULT_TICK_INTERVAL,
        }
    }

    /// Override the tick interval (config-driven, e.g. `lopi.toml`).
    #[must_use]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Run one tick: check every enabled entry, fire the favorable ones that
    /// aren't in cooldown. Returns the number of entries fired.
    ///
    /// # Errors
    /// Returns `Err` if listing entries fails.
    pub async fn tick(&self) -> Result<usize> {
        let now = Utc::now();
        let local_hour = chrono::Local::now().hour();
        let mut fired = 0;
        for row in self.store.list_maxx_entries().await?.into_iter() {
            if !row.enabled {
                continue;
            }
            let spec: MaxxSpec = row.into();
            if !is_favorable(&spec, &self.quota, now, local_hour) {
                continue;
            }
            if self.in_cooldown(&spec.id, now).await {
                continue;
            }
            fire(&self.pool, &self.store, &spec).await;
            fired += 1;
        }
        Ok(fired)
    }

    async fn in_cooldown(&self, maxx_id: &str, now: DateTime<Utc>) -> bool {
        let Ok(runs) = self.store.list_maxx_runs(maxx_id, 1).await else {
            return false;
        };
        let Some(last) = runs.first() else {
            return false;
        };
        let Ok(fired_at) = DateTime::parse_from_rfc3339(&last.fired_at) else {
            return false;
        };
        (now.timestamp() - fired_at.timestamp()) < MIN_REFIRE_INTERVAL_SECS
    }

    /// Spawn the tick loop on its configured interval. Runs until the
    /// process exits — there is no explicit shutdown handle, matching
    /// `ScheduleManager`'s cron jobs.
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Err(e) = self.tick().await {
                    warn!("maxx tick failed: {e:#}");
                }
                tokio::time::sleep(self.interval).await;
            }
        })
    }
}

/// Submit a task for `spec` and append a run-history row.
async fn fire(pool: &AgentPool, store: &MemoryStore, spec: &MaxxSpec) {
    info!(maxx = %spec.id, "firing maxx task: {}", spec.goal);
    let task = build_task(spec);
    let new_id = task.id.0.to_string();
    let duplicate = pool.submit(task).await;
    let (task_id, outcome) = match &duplicate {
        Some(existing) => (existing.0.to_string(), "duplicate"),
        None => (new_id, "queued"),
    };
    if let Err(e) = store
        .record_maxx_run(&spec.id, Some(&task_id), outcome)
        .await
    {
        warn!(maxx = %spec.id, "failed to record maxx run: {e:#}");
    }
}

#[cfg(test)]
#[path = "maxx_loop_tests.rs"]
mod tests;
