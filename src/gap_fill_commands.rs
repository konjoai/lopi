//! `lopi gap-fill` — run the test suite, identify failing spec items, and
//! auto-queue fix tasks into the lopi agent pool via the web API.
//!
//! Design: gap-fill is a CLI tool, not a daemon. It runs once, reports,
//! and queues tasks for a running `lopi sail` server. If no server is
//! running, it prints the gaps and exits without queuing.

use anyhow::Result;
use lopi_spec::{coverage_gaps, run_tests, SpecSurface};
use std::path::PathBuf;

/// Run tests, find coverage gaps, and (optionally) queue fix tasks.
pub async fn run(repo: PathBuf, sail_url: &str, dry_run: bool) -> Result<()> {
    println!("🔬 lopi gap-fill — {}", repo.display());
    println!();

    // 1. Load spec surface.
    let surface = match SpecSurface::load(&repo)? {
        Some(s) => {
            println!("  📋 spec surface: {} items (cached)", s.len());
            s
        }
        None => {
            let live = SpecSurface::extract(&repo)?;
            println!("  📋 spec surface: {} items (live)", live.len());
            live
        }
    };

    if surface.is_empty() {
        println!("  No spec items found. Run `lopi spec --save` first.");
        return Ok(());
    }

    // 2. Run tests.
    println!("  🧪 running tests…");
    let results = run_tests(&repo).await?;
    println!("  ✅ {} results captured", results.len());

    // 3. Compute coverage gaps.
    let gaps = coverage_gaps(&surface.items, &results);
    println!();

    if gaps.is_empty() {
        println!("  ✅ No coverage gaps — all spec items have passing tests.");
        return Ok(());
    }

    println!("  ⚠️  {} coverage gap(s):", gaps.len());
    for g in &gaps {
        println!("     [{kind}] {desc} ({file}:{line})",
            kind = g.kind.as_str(), desc = g.description,
            file = g.file, line = g.line
        );
    }
    println!();

    if dry_run {
        println!("  dry-run: not queuing fix tasks");
        return Ok(());
    }

    // 4. Queue fix tasks via the sail API.
    println!("  📤 queuing fix tasks on {sail_url}…");
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
                println!("     ↳ queued: {}", gap.name);
            }
            Ok(resp) => {
                tracing::warn!("queue rejected: {}", resp.status());
            }
            Err(e) => {
                tracing::warn!("queue request failed: {e}");
            }
        }
    }

    if queued == 0 {
        println!("  ⚠️  No tasks queued — is `lopi sail` running on {sail_url}?");
    } else {
        println!();
        println!("  ✅ {queued} fix task(s) queued — run `lopi watch` to monitor progress");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles() {
        // Integration test would require a running sail server; just verify compilation.
    }
}
