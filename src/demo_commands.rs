//! `lopi demo` — generate (if absent) and open a fully synthetic dashboard.
//!
//! Isolation, marking, and refusal guarantees are documented in
//! `docs/adr/0001-demo-mode-and-measurement.md` and enforced primarily by
//! `lopi-demo` (the generator refuses to write to the real store path) and
//! `lopi-ui` (mutation handlers refuse against a synthetic store, and the
//! web server's cron/quota/MAXX warm-up is skipped entirely for one). This
//! module's own job is narrow: resolve the two paths, generate if needed,
//! and launch the dashboard — never the agent pool's dispatch loop.

use anyhow::Result;
use lopi_core::{AgentEvent, EventBus, LopiConfig};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::util::{db_path, expand_home};

/// `lopi demo` entry point.
///
/// `seed`/`reset`/`off`/`path_only` mirror the CLI flags 1:1. `host`/`port`
/// are only consulted when actually launching the dashboard (i.e. neither
/// `off` nor `path_only` was requested).
///
/// # Errors
/// Returns `Err` if generation fails (including the isolation refusal when
/// the demo path resolves to the configured real store) or the dashboard
/// fails to bind.
pub async fn run(
    seed: Option<u64>,
    reset: bool,
    off: bool,
    path_only: bool,
    host: String,
    port: u16,
    cfg: Option<&LopiConfig>,
) -> Result<()> {
    let real_store = cfg.map_or_else(db_path, |c| expand_home(c.lopi.db_path.clone()));
    let demo_store = lopi_demo::default_demo_store_path();

    if path_only {
        println!("{}", demo_store.display());
        return Ok(());
    }

    if off {
        remove_demo_store(&demo_store);
        println!("🧪 demo store removed: {}", demo_store.display());
        return Ok(());
    }

    if reset {
        remove_demo_store(&demo_store);
    }

    if demo_store.exists() {
        println!(
            "🧪 demo store already present at {} — reusing (use --reset to regenerate)",
            demo_store.display()
        );
    } else {
        let seed = seed.unwrap_or(lopi_demo::DEFAULT_DEMO_SEED);
        let generated = lopi_demo::generate(&demo_store, &real_store, seed).await?;
        println!(
            "🧪 generated demo store: {} repos, {} tasks (seed {})",
            generated.repo_count, generated.task_count, generated.seed
        );
    }

    launch_dashboard(demo_store, host, port).await
}

/// Delete the demo store and its SQLite WAL/SHM sidecar files, if present.
/// Takes only `demo_store` — structurally incapable of touching the real
/// store, since it never sees that path.
fn remove_demo_store(demo_store: &Path) {
    let base = demo_store.display().to_string();
    for candidate in [
        demo_store.to_path_buf(),
        PathBuf::from(format!("{base}-wal")),
        PathBuf::from(format!("{base}-shm")),
    ] {
        if candidate.exists() {
            if let Err(e) = std::fs::remove_file(&candidate) {
                tracing::warn!(path = %candidate.display(), "failed to remove demo store file: {e}");
            }
        }
    }
}

/// Launch the web dashboard against `demo_store`. Deliberately mirrors
/// `sail_commands::run`'s shape but never spawns the agent pool's dispatch
/// loop and always runs without Bearer auth — a synthetic store has nothing
/// to protect, and `--insecure-no-auth` is refused on any non-loopback host
/// by `validate_auth_policy`, so this can't accidentally expose a public
/// demo endpoint unauthenticated (`launch_dashboard` is only ever called
/// with the CLI's own `--host` default of `127.0.0.1`, and a caller
/// deliberately overriding it to a non-loopback address hits that same
/// refusal here rather than silently going out unauthenticated).
async fn launch_dashboard(demo_store: PathBuf, host: String, port: u16) -> Result<()> {
    lopi_ui::web::validate_auth_policy(None, true, &host)?;

    let store = MemoryStore::open(&demo_store).await?;
    let bus: EventBus<AgentEvent> = EventBus::new(512);
    let queue = TaskQueue::new();
    // A pool is required to construct `AppState`, but its dispatch loop
    // (`AgentPool::run`) is never spawned below — so nothing ever picks a
    // queued demo task up, regardless of what a mutation endpoint might
    // (already refuses to) accept. See A.5 in the ADR.
    let pool = Arc::new(
        AgentPool::new(1, PathBuf::from("/demo"), queue.clone(), bus.clone())
            .with_store(store.clone()),
    );

    print_startup_banner(&demo_store, &host, port);

    if std::env::var("LOPI_NO_BROWSER").ok().as_deref() != Some("1") {
        let url = dashboard_url(&host, port);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(350)).await;
            open_dashboard(&url);
        });
    }

    lopi_ui::web::serve_with_repo(
        store,
        bus,
        queue,
        pool,
        &host,
        port,
        None,
        PathBuf::from("/demo"),
        Vec::new(),
        None,
    )
    .await
}

fn dashboard_url(host: &str, port: u16) -> String {
    let host = match host {
        "0.0.0.0" | "::" => "127.0.0.1",
        h => h,
    };
    format!("http://{host}:{port}")
}

fn open_dashboard(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).status();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).status();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = url;
    }
}

fn print_startup_banner(demo_store: &Path, host: &str, port: u16) {
    println!("🧪 lopi demo — synthetic data, watermarked on every surface");
    println!("   store:     {}", demo_store.display());
    println!("   dashboard: http://{host}:{port}");
    println!("   api:       http://{host}:{port}/api/tasks");
    println!("   tui:       lopi watch --demo (in another terminal)");
    println!("   off:       lopi demo --off");
    println!();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn remove_demo_store_deletes_main_and_sidecar_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let demo = dir.path().join("demo.db");
        let real = dir.path().join("lopi.db");
        for p in [&demo, &real] {
            std::fs::write(p, b"x").unwrap();
        }
        std::fs::write(dir.path().join("demo.db-wal"), b"x").unwrap();
        std::fs::write(dir.path().join("demo.db-shm"), b"x").unwrap();

        remove_demo_store(&demo);

        assert!(!demo.exists(), "demo store removed");
        assert!(
            !dir.path().join("demo.db-wal").exists(),
            "wal sidecar removed"
        );
        assert!(
            !dir.path().join("demo.db-shm").exists(),
            "shm sidecar removed"
        );
        assert!(real.exists(), "real store untouched");
    }

    #[test]
    fn remove_demo_store_on_absent_file_is_a_silent_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let demo = dir.path().join("does-not-exist.db");
        remove_demo_store(&demo);
        assert!(!demo.exists());
    }

    #[tokio::test]
    async fn run_off_never_touches_the_real_store() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("lopi.db");
        std::fs::write(&real, "real data — must survive").unwrap();
        let real_bytes_before = std::fs::read(&real).unwrap();

        // Point HOME at a scratch dir so `default_demo_store_path()` doesn't
        // touch the developer's actual `~/.lopi/demo.db`.
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", home.path());

        let cfg: LopiConfig = serde_json::from_value(serde_json::json!({
            "lopi": { "db_path": real.display().to_string() },
            "claude": {}, "git": {}
        }))
        .unwrap();

        run(None, false, true, false, "127.0.0.1".into(), 0, Some(&cfg))
            .await
            .unwrap();

        let real_bytes_after = std::fs::read(&real).unwrap();
        assert_eq!(
            real_bytes_before, real_bytes_after,
            "--off must leave the real store byte-identical"
        );
    }

    #[test]
    fn dashboard_url_maps_wildcard_hosts_to_loopback() {
        assert_eq!(dashboard_url("0.0.0.0", 3000), "http://127.0.0.1:3000");
        assert_eq!(dashboard_url("127.0.0.1", 3000), "http://127.0.0.1:3000");
    }
}
