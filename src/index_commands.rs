//! `lopi index` / `lopi map` — build/refresh the per-repo symbol index
//! (`.lopi/index.db`) and print the deterministic repo map that
//! `runner/seed.rs` injects into the planning prompt under
//! `context.mode = "index"`.

use anyhow::{Context, Result};
use lopi_core::RepoProfile;
use lopi_index::{IndexConfig, IndexStore, RepoMap, INDEX_DB_REL_PATH};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Reindex `repo` and print the resulting delta + wall time — the number
/// the LEDGER's "under 150ms for a one-file change" target is measured
/// against.
///
/// # Errors
/// Returns `Err` if the index database can't be opened or the reindex pass fails.
pub async fn run_index(repo: PathBuf) -> Result<()> {
    let repo = repo.canonicalize().context("resolving repo path")?;
    let store = open_store(&repo).await?;
    let repo_id = repo_id_for(&repo);

    let start = Instant::now();
    let delta = lopi_index::reindex::reindex(&store, &repo_id, &repo).await?;
    let elapsed = start.elapsed();

    println!("lopi-index: reindexed {}", repo.display());
    println!(
        "  files: {} indexed, {} removed, {} parse failures",
        delta.files_indexed, delta.files_removed, delta.parse_failures
    );
    println!(
        "  symbols: +{} -{}   refs: +{} -{}",
        delta.symbols_added, delta.symbols_removed, delta.refs_added, delta.refs_removed
    );
    println!(
        "  total: {} symbols, {} refs",
        store.symbol_count(&repo_id).await?,
        store.ref_count(&repo_id).await?
    );
    println!("  elapsed: {:.1}ms", elapsed.as_secs_f64() * 1000.0);
    Ok(())
}

/// Build and print the repo map for `repo` — what a `context.mode =
/// "index"` planning prompt actually sees.
///
/// # Errors
/// Returns `Err` if the index database can't be opened or the map build fails.
pub async fn run_map(repo: PathBuf) -> Result<()> {
    let repo = repo.canonicalize().context("resolving repo path")?;
    let store = open_store(&repo).await?;
    let repo_id = repo_id_for(&repo);
    let cfg = IndexConfig::default();
    let profile = RepoProfile::load_from_repo(&repo);
    let build_cmd = "cargo build".to_string();
    let test_cmd = profile
        .test_command
        .unwrap_or_else(|| "cargo test --workspace".to_string());
    let lint_cmd = profile
        .lint_command
        .unwrap_or_else(|| "cargo clippy -- -D warnings".to_string());
    let cmds = [("build", build_cmd.as_str()), ("test", test_cmd.as_str()), ("lint", lint_cmd.as_str())];

    let map = RepoMap::build(
        &store,
        &repo_id,
        &repo,
        cfg.skeleton_depth,
        cfg.top_referenced_symbols,
        &cmds,
        cfg.map_token_budget,
    )
    .await?;
    print!("{}", map.text);
    Ok(())
}

/// Open (creating on first use) the per-repo index database.
async fn open_store(repo: &Path) -> Result<IndexStore> {
    IndexStore::open(repo.join(INDEX_DB_REL_PATH)).await
}

/// A stable per-repo identifier — the canonicalized repo root path. Not
/// meant to be portable across machines (a `.lopi/index.db` is per-repo and
/// gitignored, never shared), only stable across repeated calls on this one.
fn repo_id_for(repo: &Path) -> String {
    repo.to_string_lossy().to_string()
}
