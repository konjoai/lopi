//! `lopi gap-fill` — run the test suite, identify failing spec items,
//! persist quality results to SQLite, and auto-queue fix tasks.

use anyhow::Result;
use lopi_memory::{MemoryStore, QualityRunRecord};
use lopi_spec::{coverage_gaps, run_tests, SpecSurface};
use std::path::PathBuf;

use crate::db_path;

/// Run tests, persist the quality run, find gaps, and (optionally) queue tasks.
pub async fn run(
    repo: PathBuf,
    sail_url: &str,
    dry_run: bool,
    quiet: bool,
) -> Result<QualitySnapshot> {
    /* mutants::skip — integration handler: requires live SpecSurface, MemoryStore, and sail API */
    if !quiet {
        println!("🔬 lopi gap-fill — {}", repo.display());
        println!();
    }

    // 1. Load or extract spec surface.
    let surface = match SpecSurface::load(&repo)? {
        Some(s) => {
            if !quiet {
                println!("  📋 spec surface: {} items (cached)", s.len());
            }
            s
        }
        None => {
            let live = SpecSurface::extract(&repo)?;
            if !quiet {
                println!("  📋 spec surface: {} items (live)", live.len());
            }
            live
        }
    };

    if surface.is_empty() {
        if !quiet {
            println!("  No spec items found. Run `lopi spec --save` first.");
        }
        return Ok(QualitySnapshot::empty(repo.to_string_lossy().to_string()));
    }

    // 2. Run tests.
    if !quiet {
        println!("  🧪 running tests…");
    }
    let results = run_tests(&repo).await?;
    let passing = results.iter().filter(|r| r.passed).count();
    let failing = results.iter().filter(|r| !r.passed).count();
    if !quiet {
        println!(
            "  ✅ {} results: {} passing, {} failing",
            results.len(),
            passing,
            failing
        );
    }

    // 3. Compute coverage gaps.
    let gaps = coverage_gaps(&surface.items, &results);
    let gap_count = gaps.len();

    // 4. Persist quality run.
    let store = MemoryStore::open(db_path()).await?;
    let run_id = store
        .save_quality_run(QualityRunRecord {
            repo_path: repo.to_string_lossy().to_string(),
            spec_items: surface.len(),
            passing,
            failing,
            gaps: gap_count,
        })
        .await?;

    // 5. Print trend delta.
    if !quiet {
        if let Ok(Some((latest, prev))) = store.quality_trend_delta(&repo.to_string_lossy()).await {
            let arrow = if latest > prev {
                "↑"
            } else if latest < prev {
                "↓"
            } else {
                "→"
            };
            println!(
                "  📈 coverage: {:.0}% {arrow} (was {:.0}%)",
                latest * 100.0,
                prev * 100.0
            );
        } else {
            let score = if surface.is_empty() {
                0.0
            } else {
                passing as f64 / surface.len() as f64
            };
            println!("  📈 coverage: {:.0}%", score * 100.0);
        }
        println!();
    }

    let snapshot = QualitySnapshot {
        run_id,
        repo_path: repo.to_string_lossy().to_string(),
        spec_items: surface.len(),
        passing,
        failing,
        gaps: gap_count,
    };

    if gaps.is_empty() {
        if !quiet {
            println!("  ✅ No coverage gaps — all spec items have passing tests.");
        }
        return Ok(snapshot);
    }

    if !quiet {
        println!("  ⚠️  {} coverage gap(s):", gap_count);
        for g in &gaps {
            println!(
                "     [{kind}] {desc} ({file}:{line})",
                kind = g.kind.as_str(),
                desc = g.description,
                file = g.file,
                line = g.line
            );
        }
        println!();
    }

    if dry_run {
        if !quiet {
            println!("  dry-run: not queuing fix tasks");
        }
        return Ok(snapshot);
    }

    // 6. Queue fix tasks via the sail API.
    if !quiet {
        println!("  📤 queuing fix tasks on {sail_url}…");
    }
    let client = reqwest::Client::new();
    let mut queued = 0usize;

    for gap in &gaps {
        let goal = format!(
            "Fix failing or missing test in {}: {} ({}:{})",
            repo.display(),
            gap.description,
            gap.file,
            gap.line,
        );
        let body = serde_json::json!({ "goal": goal, "priority": "normal" });
        let url = format!("{sail_url}/api/tasks");
        match client.post(&url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                queued += 1;
                if !quiet {
                    println!("     ↳ queued: {}", gap.name);
                }
            }
            Ok(resp) => tracing::warn!("queue rejected: {}", resp.status()),
            Err(e) => tracing::warn!("queue request failed: {e}"),
        }
    }

    if !quiet {
        if queued == 0 {
            println!("  ⚠️  No tasks queued — is `lopi sail` running on {sail_url}?");
        } else {
            println!();
            println!("  ✅ {queued} fix task(s) queued — run `lopi watch` to monitor progress");
        }
    }

    Ok(snapshot)
}

/// Summary of a single gap-fill run — returned so callers (e.g. the daemon)
/// can log or react to the result without re-querying the DB.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct QualitySnapshot {
    pub run_id: String,
    pub repo_path: String,
    pub spec_items: usize,
    pub passing: usize,
    pub failing: usize,
    pub gaps: usize,
}

impl QualitySnapshot {
    fn empty(repo_path: String) -> Self {
        Self {
            run_id: String::new(),
            repo_path,
            spec_items: 0,
            passing: 0,
            failing: 0,
            gaps: 0,
        }
    }

    #[must_use]
    pub fn score(&self) -> f64 {
        if self.spec_items == 0 {
            0.0
        } else {
            self.passing as f64 / self.spec_items as f64
        }
    }
}

/// Daemon loop: run gap-fill every `interval_minutes`, persisting trend data
/// and queuing fix tasks for each new coverage gap found.
pub async fn watch_loop(
    repo: PathBuf,
    interval_minutes: u64,
    sail_url: &str,
    run_now: bool,
) -> anyhow::Result<()> {
    /* mutants::skip — integration handler: runs indefinitely with real timer and sail API */
    let interval = tokio::time::Duration::from_secs(interval_minutes * 60);
    println!(
        "🔄 lopi watch-gap-fill — {} every {interval_minutes} min",
        repo.display()
    );
    println!("   sail: {sail_url}");
    println!("   press Ctrl-C to stop");
    println!();

    if run_now {
        let snap = run(repo.clone(), sail_url, false, true).await?;
        log_snapshot(&snap);
    }

    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await; // consume immediate first tick when run_now=false

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match run(repo.clone(), sail_url, false, true).await {
                    Ok(snap) => log_snapshot(&snap),
                    Err(e) => tracing::warn!("gap-fill iteration failed: {e}"),
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("🛑 lopi watch-gap-fill stopped");
                break;
            }
        }
    }
    Ok(())
}

fn log_snapshot(snap: &QualitySnapshot) {
    let score = snap.score();
    let arrow = if snap.gaps == 0 { "✅" } else { "⚠️ " };
    println!(
        "  {arrow} {:.0}% coverage — {} spec items, {} gaps [run {}]",
        score * 100.0,
        snap.spec_items,
        snap.gaps,
        if snap.run_id.is_empty() {
            "—"
        } else {
            &snap.run_id[..8.min(snap.run_id.len())]
        }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_score_zero_when_empty() {
        let s = QualitySnapshot::empty("/r".into());
        assert!((s.score() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn snapshot_score_correct() {
        let s = QualitySnapshot {
            run_id: "x".into(),
            repo_path: "/r".into(),
            spec_items: 10,
            passing: 8,
            failing: 2,
            gaps: 2,
        };
        assert!((s.score() - 0.8).abs() < 0.001);
    }
}
