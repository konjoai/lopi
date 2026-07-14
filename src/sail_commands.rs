use anyhow::Result;
use lopi_core::{AgentEvent, EventBus, LopiConfig};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::util::{db_path, expand_home};

pub async fn run(
    max_agents: usize,
    repo: PathBuf,
    extra_repos: Vec<PathBuf>,
    host: String,
    port: u16,
    cfg: Option<&LopiConfig>,
) -> Result<()> {
    // Honor the configured db_path when a config was loaded (via `--config` or
    // the standard search), falling back to the default location otherwise.
    // Previously this ignored `cfg` entirely, so `--config` with a custom
    // db_path silently wrote to `~/.lopi/lopi.db` and left the configured file
    // at 0 bytes — a data-isolation footgun (Ops-2 bug #6).
    let db = cfg.map_or_else(db_path, |c| expand_home(c.lopi.db_path.clone()));
    let store = MemoryStore::open(&db).await?;
    let bus: EventBus<AgentEvent> = EventBus::new(512);
    let queue = TaskQueue::new();

    // Tier gating: if LOPI_CUSTOMER_ID is set, cap max_agents to the tier
    // stored in the DB. This lets Stripe subscription events take effect on
    // the next `lopi sail` restart without a code change.
    let effective_max_agents = tier_capped_max_agents(&store, max_agents).await;

    let pool = Arc::new(
        AgentPool::new(
            effective_max_agents,
            repo.clone(),
            queue.clone(),
            bus.clone(),
        )
        .with_store(store.clone()),
    );

    print_startup_banner(effective_max_agents, &repo, &extra_repos, &host, port);

    // Spawn additional per-repo dispatch loops for multi-repo mode.
    // Each extra repo shares the same queue and bus; the pool routes by
    // task.repo_path, so tasks land on the right worktree.
    for extra in &extra_repos {
        let extra_pool = AgentPool::new(
            effective_max_agents,
            extra.clone(),
            queue.clone(),
            bus.clone(),
        )
        .with_store(store.clone());
        tokio::spawn(async move {
            if let Err(e) = extra_pool.run().await {
                tracing::error!("multi-repo pool error: {e}");
            }
        });
    }

    let schedules = cfg.map(|c| c.schedules.clone()).unwrap_or_default();
    if !schedules.is_empty() {
        println!("   schedules: {} seeded from lopi.toml", schedules.len());
        // Seed TOML schedules into the durable store (idempotent, matched by
        // name) so they appear in the dashboard cron UI. The web layer's
        // `ScheduleManager::start()` then registers all enabled rows as live
        // jobs — replacing the old static `boot_scheduler` path.
        seed_schedules(&store, &schedules).await;
    }
    println!();

    let pool_for_dispatch = (*pool).clone();
    tokio::spawn(async move {
        if let Err(e) = pool_for_dispatch.run().await {
            tracing::error!("pool error: {e}");
        }
    });

    let pool_handle = (*pool).clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down — cancelling all running agents");
        pool_handle.shutdown().await;
        std::process::exit(0);
    });

    if let Ok(token) = std::env::var("TELOXIDE_TOKEN") {
        let pool_for_tg = (*pool).clone();
        let schedules_for_tg = schedules.clone();
        spawn_telegram(
            token,
            queue.clone(),
            store.clone(),
            pool_for_tg,
            bus.clone(),
            schedules_for_tg,
            cfg,
        );
    }

    let auth_token = cfg.and_then(|c| c.web.auth_token.clone());

    // Open the dashboard in the user's browser once the server is up.
    // Honors LOPI_NO_BROWSER=1 for headless / remote deployments.
    if std::env::var("LOPI_NO_BROWSER").ok().as_deref() != Some("1") {
        let url = dashboard_url(&host, port);
        tokio::spawn(async move {
            // Server bind happens synchronously below `serve_with_repo`; a
            // short delay covers the listener handshake without polling.
            tokio::time::sleep(std::time::Duration::from_millis(350)).await;
            open_dashboard(&url);
        });
    }

    // Pass the effective config so `GET /api/config` reflects what's actually
    // loaded rather than re-discovering a file independently (Ops-2 bug #6).
    lopi_ui::web::serve_with_repo(
        store,
        bus,
        queue,
        pool,
        &host,
        port,
        auth_token,
        repo,
        extra_repos,
        cfg.cloned(),
    )
    .await
}

/// Build the dashboard URL, mapping wildcard bind addresses to a routable
/// loopback host so the browser doesn't try to connect to 0.0.0.0.
fn dashboard_url(host: &str, port: u16) -> String {
    let host = match host {
        "0.0.0.0" | "::" => "127.0.0.1",
        h => h,
    };
    format!("http://{host}:{port}")
}

/// Open the dashboard. On macOS, first try to find an existing tab in a
/// Chromium-family browser via AppleScript — if found, activate that tab
/// and reload it instead of opening a new one. Falls back to the OS's
/// default-browser open command on any failure or non-mac platform.
fn open_dashboard(url: &str) {
    #[cfg(target_os = "macos")]
    {
        if !focus_existing_tab_macos(url) {
            let _ = std::process::Command::new("open").arg(url).status();
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .status();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = url; // unsupported platform — leave the dashboard for the user
    }
}

/// Walk Chrome / Brave / Arc / Edge windows looking for a tab whose URL
/// matches `url`. If found, activate the browser, raise the window, focus
/// the tab, and reload. Returns true when an existing tab was reused.
///
/// Safari and Firefox use different scripting dictionaries, so we let the
/// `open` fallback handle them — every Chromium-family browser shares the
/// `windows / tabs / URL / reload` vocabulary used here.
#[cfg(target_os = "macos")]
fn focus_existing_tab_macos(url: &str) -> bool {
    // The AppleScript prints "REUSED" to stdout when a matching tab was
    // found and refreshed. Otherwise it prints nothing.
    let script = format!(
        r#"
set targetURL to "{url}"
set browserNames to {{"Google Chrome", "Brave Browser", "Arc", "Microsoft Edge", "Chromium"}}
repeat with bname in browserNames
    set isRunning to false
    try
        tell application "System Events" to set isRunning to (exists (processes where name is bname))
    end try
    if isRunning then
        try
            using terms from application "Google Chrome"
                tell application bname
                    set winIndex to 0
                    repeat with w in windows
                        set winIndex to winIndex + 1
                        set tabIndex to 0
                        repeat with t in tabs of w
                            set tabIndex to tabIndex + 1
                            if (URL of t) starts with targetURL then
                                set active tab index of w to tabIndex
                                set index of w to 1
                                tell t to reload
                                activate
                                log "REUSED"
                                return "REUSED"
                            end if
                        end repeat
                    end repeat
                end tell
            end using terms from
        end try
    end if
end repeat
return ""
"#
    );
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output();
    match out {
        Ok(o) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            combined.contains("REUSED")
        }
        Err(_) => false,
    }
}

fn print_startup_banner(
    max_agents: usize,
    repo: &Path,
    extra_repos: &[PathBuf],
    host: &str,
    port: u16,
) {
    println!("🚢 lopi sail");
    println!("   agents:    up to {max_agents} concurrent");
    println!("   repo:      {}", repo.display());
    for r in extra_repos {
        println!("   + repo:    {}", r.display());
    }
    println!("   dashboard: http://{host}:{port}");
    println!("   api:       http://{host}:{port}/api/tasks");
    println!("   ws:        ws://{host}:{port}/ws");
}

/// Return `max_agents` capped by the `CustomerTier` stored in the DB.
///
/// Reads `LOPI_CUSTOMER_ID` from the environment. When the variable is absent,
/// or the customer has no active installation, the requested value is returned
/// unchanged (i.e., tier gating is opt-in).
async fn tier_capped_max_agents(store: &MemoryStore, requested: usize) -> usize {
    let Some(customer_id) = std::env::var("LOPI_CUSTOMER_ID").ok() else {
        return requested;
    };
    match store.customer_tier(&customer_id).await {
        Ok(tier) => {
            let cap = tier.max_agents();
            if requested > cap {
                tracing::info!(
                    customer_id,
                    requested,
                    cap,
                    tier = %tier,
                    "max_agents capped by subscription tier"
                );
                cap
            } else {
                requested
            }
        }
        Err(e) => {
            tracing::warn!(customer_id, "failed to read customer tier: {e}");
            requested
        }
    }
}

/// Insert any `lopi.toml` schedule into the durable `schedules` table that is
/// not already present (matched by name). Idempotent across restarts so a
/// checked-in config keeps working while the cron UI manages its own rows.
async fn seed_schedules(store: &MemoryStore, entries: &[lopi_core::ScheduleEntry]) {
    for entry in entries {
        match store.find_schedule_by_name(&entry.name).await {
            Ok(Some(_)) => {} // already seeded — leave UI edits intact
            Ok(None) => {
                let input = lopi_memory::ScheduleInput {
                    id: None,
                    name: entry.name.clone(),
                    cron: entry.cron.clone(),
                    goal: entry.goal.clone(),
                    repo: Some(entry.repo.display().to_string()),
                    priority: entry.priority.clone(),
                    allowed_dirs: entry.allowed_dirs.clone(),
                    forbidden_dirs: entry.forbidden_dirs.clone(),
                    enabled: true,
                    autonomy_level: entry.autonomy_level.tag_snake().to_string(),
                };
                if let Err(e) = store.upsert_schedule(&input).await {
                    tracing::warn!(schedule = %entry.name, "seeding schedule failed: {e:#}");
                }
            }
            Err(e) => tracing::warn!(schedule = %entry.name, "schedule lookup failed: {e:#}"),
        }
    }
}

fn spawn_telegram(
    token: String,
    queue: TaskQueue,
    store: MemoryStore,
    pool: AgentPool,
    bus: EventBus<AgentEvent>,
    schedules: Vec<lopi_core::ScheduleEntry>,
    cfg: Option<&LopiConfig>,
) {
    let allowed_chat_ids = cfg
        .map(|c| c.remote.telegram.allowed_chat_ids.clone())
        .unwrap_or_default();
    let notify_chat_id = cfg.and_then(|c| c.remote.telegram.chat_id);
    tokio::spawn(async move {
        if let Err(e) = lopi_remote::telegram::run(
            token,
            queue,
            store,
            pool,
            bus,
            schedules,
            notify_chat_id,
            allowed_chat_ids,
        )
        .await
        {
            tracing::error!("telegram bot error: {e}");
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_core::{AutonomyLevel, ScheduleEntry};

    fn entry(name: &str) -> ScheduleEntry {
        ScheduleEntry {
            name: name.into(),
            repo: PathBuf::from("/tmp/repo"),
            goal: "run nightly checks".into(),
            cron: "0 2 * * *".into(),
            priority: "high".into(),
            allowed_dirs: vec!["src/".into()],
            forbidden_dirs: vec!["infra/".into()],
            autonomy_level: AutonomyLevel::VerifiedPr,
            report: None,
        }
    }

    #[tokio::test]
    async fn seed_schedules_persists_new_entries_with_autonomy() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        seed_schedules(&store, &[entry("nightly")]).await;
        let row = store
            .find_schedule_by_name("nightly")
            .await
            .unwrap()
            .expect("seeded row present");
        assert_eq!(row.goal, "run nightly checks");
        assert_eq!(row.autonomy_level, "verified_pr");
        assert!(row.enabled);
    }

    #[tokio::test]
    async fn seed_schedules_is_idempotent_by_name() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        seed_schedules(&store, &[entry("dup")]).await;
        seed_schedules(&store, &[entry("dup")]).await;
        assert_eq!(store.list_schedules().await.unwrap().len(), 1);
    }
}
