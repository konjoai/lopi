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

    if should_open_browser() {
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

/// Whether `launch_dashboard` should try to open a browser tab — `false`
/// only when `LOPI_NO_BROWSER=1` is set (headless/remote deployments). Pure
/// so the condition is unit-testable without spawning a real browser.
fn should_open_browser() -> bool {
    std::env::var("LOPI_NO_BROWSER").ok().as_deref() != Some("1")
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

    /// `HOME`/`LOPI_NO_BROWSER` are process-global; `cargo test` runs tests
    /// as threads within one process, so every test that mutates either must
    /// hold this guard for its whole body or a parallel thread's `set_var`
    /// can interleave and read/write the wrong scratch state.
    static ENV_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        let _guard = ENV_GUARD.lock().await;
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

    /// End-to-end proof of A.5/A.9's "no agent spawn, git call, or network
    /// call occurs in demo mode": generate a real demo store, serve it
    /// in-process (the exact `launch_dashboard` path `lopi demo` uses,
    /// never `AgentPool::run`), attempt a mutation over HTTP, and confirm
    /// the store's task/attempt counts are unchanged afterward — nothing
    /// consumed the (refused) request, and nothing in the demo store's
    /// content silently grew from a background dispatch loop that was
    /// never started.
    #[tokio::test]
    async fn serving_a_demo_store_never_dispatches_or_accepts_mutations() {
        let _guard = ENV_GUARD.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let demo = dir.path().join("demo.db");
        let real = dir.path().join("lopi.db");
        lopi_demo::generate(&demo, &real, 7).await.unwrap();

        let store = MemoryStore::open(&demo).await.unwrap();
        assert!(store.is_synthetic().await.unwrap());
        let tasks_before = store.task_count().await.unwrap();
        let attempts_before = store.status_counts().await.unwrap();

        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        std::env::set_var("LOPI_NO_BROWSER", "1");
        tokio::spawn(launch_dashboard(demo.clone(), "127.0.0.1".into(), port));

        let base = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();
        for _ in 0..100 {
            if client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let resp = client
            .post(format!("{base}/api/tasks"))
            .json(&serde_json::json!({ "goal": "an attacker-controlled goal string" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 403, "demo mode must refuse task creation");
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["synthetic"], true);

        // Give any (hypothetical) background dispatch loop a real chance to
        // act, then confirm nothing changed — the strongest available
        // evidence short of asserting `claude`/`git` were never on $PATH.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let store2 = MemoryStore::open(&demo).await.unwrap();
        assert_eq!(
            store2.task_count().await.unwrap(),
            tasks_before,
            "no task was created despite the (refused) POST"
        );
        let attempts_after = store2.status_counts().await.unwrap();
        assert_eq!(
            (
                attempts_before.running,
                attempts_before.succeeded,
                attempts_before.failed
            ),
            (
                attempts_after.running,
                attempts_after.succeeded,
                attempts_after.failed
            ),
            "no background dispatch changed any task's lifecycle bucket"
        );
    }

    #[test]
    fn dashboard_url_maps_wildcard_hosts_to_loopback() {
        assert_eq!(dashboard_url("0.0.0.0", 3000), "http://127.0.0.1:3000");
        assert_eq!(dashboard_url("127.0.0.1", 3000), "http://127.0.0.1:3000");
    }

    /// Mutation-testing kill test for `should_open_browser`'s `!=`
    /// comparison: pins both branches directly so a mutant flipping it to
    /// `==` fails immediately, without needing to observe a real browser
    /// launch attempt.
    #[tokio::test]
    async fn should_open_browser_only_false_when_explicitly_suppressed() {
        let _guard = ENV_GUARD.lock().await;
        std::env::set_var("LOPI_NO_BROWSER", "1");
        assert!(!should_open_browser(), "suppressed via LOPI_NO_BROWSER=1");
        std::env::set_var("LOPI_NO_BROWSER", "0");
        assert!(should_open_browser(), "any other value does not suppress");
        std::env::remove_var("LOPI_NO_BROWSER");
        assert!(should_open_browser(), "unset does not suppress");
    }

    /// Mutation-testing kill test for `run`'s whole-body mutant (`replace
    /// run -> Result<()> with Ok(())`): a non-loopback host makes
    /// `launch_dashboard`'s `validate_auth_policy` call fail fast (demo mode
    /// never passes real auth, so any non-loopback host is refused) instead
    /// of blocking forever in `serve_with_repo` — so the full
    /// generate-if-absent-then-launch path is exercisable synchronously.
    /// Real `run` returns that `Err` and leaves the freshly-generated demo
    /// store on disk; a `Ok(())`-stub mutant would return `Ok(())` and
    /// (having never run the real body) leave no store behind either —
    /// both observably distinguish it from the real function.
    #[tokio::test]
    async fn run_generates_then_surfaces_the_launch_failure_on_a_bad_host() {
        let _guard = ENV_GUARD.lock().await;
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", home.path());
        let demo_store = lopi_demo::default_demo_store_path();
        assert!(!demo_store.exists(), "clean scratch HOME");

        let result = run(
            Some(99),
            false,
            false,
            false,
            "203.0.113.1".into(), // TEST-NET-3, never loopback — refused fast
            0,
            None,
        )
        .await;

        assert!(
            result.is_err(),
            "a non-loopback host must be refused, not silently succeed"
        );
        assert!(
            demo_store.exists(),
            "the demo store must have been generated before the launch attempt failed"
        );
    }
}
