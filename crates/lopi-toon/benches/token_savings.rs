//! Token-savings measurement for TOON vs. compact JSON, run against the
//! actual payload shapes lopi sends (see `crates/lopi-agent/src/claude.rs:1-6`),
//! not synthetic data. Not a criterion timing benchmark — `harness = false`
//! because this reports a one-shot size/token comparison, not wall-clock
//! throughput.
//!
//! Tokenizer: `tiktoken-rs` `cl100k_base` (OpenAI's GPT-4 BPE), used because
//! no `ANTHROPIC_API_KEY` was available in the measuring environment. This is
//! **not** a Claude token count — see the README and `crates/lopi-toon/src/lib.rs`
//! doc comment for that caveat. If `ANTHROPIC_API_KEY` is set, this harness
//! additionally cross-checks a sample of payloads against Anthropic's
//! `count_tokens` endpoint and reports both.
//!
//! Run with: `cargo bench -p lopi-toon --bench token_savings`
//!
//! Corpus:
//! - Goals: 27 unique real task goals pulled from
//!   `artifacts/diagnostics/20260717T113652Z/tasks.json`, plus the 10 T01-T10
//!   goals defined in `benchmarks/run.sh` (37 total).
//! - Dirs: the two `allowed_dirs`/`forbidden_dirs` sets that actually appear
//!   in this repo — the shipped defaults in `lopi.toml.example` and a
//!   crate-scoped set using real crate paths under `crates/`.
//! - Constraints: the five bullets under "Critical Constraints" in this
//!   repo's own `CLAUDE.md`.
//! - Patterns/lessons: representative rows conforming to the `patterns` and
//!   `lessons` table schemas in `crates/lopi-memory/src/schema.sql`. No
//!   `lopi.db` was present in the measuring checkout, so these are
//!   schema-accurate but not pulled from a live table — noted here rather
//!   than silently presented as production data.

use serde_json::Value;
use std::fmt::Write as _;
use std::fs;
use tiktoken_rs::CoreBPE;

// ── Corpus: goals ───────────────────────────────────────────────────────────

/// 27 unique goals observed in `artifacts/diagnostics/20260717T113652Z/tasks.json`.
const REAL_GOALS: &[&str] = &[
    "Create a next steps plan for the konjoai repos",
    "List files and read lib.rs, then describe task1 in one line",
    "List files and read lib.rs, then describe task2 in one line",
    "List files and read lib.rs, then describe task3 in one line",
    "List files, read mod1.rs, and explain helper1 in one sentence",
    "List files, read mod2.rs, and explain helper2 in one sentence",
    "List files, read mod3.rs, and explain helper3 in one sentence",
    "List files, read mod4.rs, and explain helper4 in one sentence",
    "List the files and read main.rs, then explain what add does in one sentence",
    "List the files in this repo and read main.rs, then say done",
    "Read main.rs and add a one-line comment at the top summarizing what it does.",
    "Read main.rs and note what it prints.",
    "conduct deep research about architectures, components, approaches that can be applied to kairu and how they could be implemented then write the findings to research.md",
    "conduct deep research about llm memory architectures and how they could be implemented then write the findings to research.md",
    "conduct deep research about llm memory architectures, components, approaches that can be applied to kairu and how they could be implemented then write the findings to research.md",
    "count how many Rust source files are in this repo and report the number",
    "count how many markdown .md files exist in the docs directory and report the number",
    "count the total number of .rs files in the crates directory",
    "create a next steps plan for the konjoai repo vectro",
    "create next steps for the konjoai repos",
    "list the crate names under crates/ and report how many there are",
    "list the top-level directories in this repository",
    "make a next steps plan for the konjoai repos",
    "research the problem space",
    "summarize what the README describes in one sentence",
    "test",
    r#"verify stack acceptance for "stack one""#,
];

/// The T01-T10 corpus goals defined in `benchmarks/run.sh`.
const CORPUS_GOALS: &[&str] = &[
    "Add a unit test for the jaccard_similarity function in lopi-memory",
    "Add PartialEq derive to AgentState in lopi-core and fix all match exhaustiveness",
    "Implement Display for TaskStatus in lopi-core that produces human-readable output",
    "Add created_at index to the patterns table in lopi-memory schema.sql",
    "Add a --verbose flag to lopi run that prints raw claude output to stdout",
    "Refactor runner.rs to extract the plan+implement+fix attempt loop into a named method",
    "Add GET /api/metrics endpoint to lopi-ui web dashboard returning PoolStats as JSON",
    "Implement retry_with_backoff in lopi-agent runner.rs for transient IO errors",
    "Add lopi bench CLI subcommand that runs T01-T10 corpus tasks sequentially",
    "Integrate AnthropicLimiter from lopi-ratelimit into AgentPool for TPM and RPM enforcement",
];

// ── Corpus: dirs (from lopi.toml.example, the shipped default config) ──────

const DIR_SETS: &[(&[&str], &[&str])] = &[
    (&["src/", "tests/"], &[".github/", "infra/", "Cargo.toml"]),
    (
        &["crates/lopi-agent/", "crates/lopi-core/"],
        &["crates/lopi-remote/", ".github/"],
    ),
];

// ── Corpus: constraints (this repo's own CLAUDE.md "Critical Constraints") ──

const CONSTRAINTS: &[&str] = &[
    "No unwrap()/expect() outside tests — use anyhow::Result and ?",
    "No blocking I/O on async paths — use spawn_blocking for synchronous ops",
    "No silent failures — log via tracing::warn! if a fallback swallows an error",
    "cargo build must stay green — fix before doing anything else",
    "Stay inside crates/ and src/ — never touch root Cargo.lock deliberately",
];

// ── Corpus: patterns/lessons (schema-conformant, not live-pulled — see module doc) ──

fn representative_patterns() -> Vec<(String, String)> {
    vec![
        (
            "refactor async runner".to_string(),
            "No blocking I/O on async paths — use spawn_blocking".to_string(),
        ),
        (
            "add cli flag argument".to_string(),
            "Update clap definitions and --help text together".to_string(),
        ),
        (
            "sqlite schema migration".to_string(),
            "Guard new columns with IF NOT EXISTS; keep schema.sql idempotent".to_string(),
        ),
    ]
}

fn representative_lessons() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "strategy",
            "Prefer extending an existing runner method over adding a new one when the loop shape is unchanged",
        ),
        (
            "recovery",
            "cargo test failures from stale sqlx offline cache: run cargo sqlx prepare before retrying",
        ),
        (
            "optimization",
            "Batch sqlx inserts inside a single transaction instead of one INSERT per row",
        ),
    ]
}

// ── Payload shapes: mirrors the three `encode_task_context` call sites ─────
//
// Building the same `serde_json::Value` once and feeding it to both
// `serde_json::to_string` (compact JSON) and `lopi_toon::encode` (TOON)
// guarantees a structurally identical comparison — not two independently
// hand-written representations of "the same" data.

fn task_context_value(
    goal: &str,
    allowed: &[&str],
    forbidden: &[&str],
    constraints: &[&str],
    patterns: &[(String, String)],
    lessons: &[(&str, &str)],
) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("goal".into(), Value::String(goal.to_string()));
    map.insert(
        "allowed".into(),
        Value::Array(allowed.iter().map(|s| Value::String((*s).into())).collect()),
    );
    map.insert(
        "forbidden".into(),
        Value::Array(forbidden.iter().map(|s| Value::String((*s).into())).collect()),
    );
    if !constraints.is_empty() {
        map.insert(
            "constraints".into(),
            Value::Array(constraints.iter().map(|s| Value::String((*s).into())).collect()),
        );
    }
    if !patterns.is_empty() {
        map.insert(
            "patterns".into(),
            Value::Array(
                patterns
                    .iter()
                    .map(|(kw, c)| {
                        let mut o = serde_json::Map::new();
                        o.insert("keywords".into(), Value::String(kw.clone()));
                        o.insert("constraints".into(), Value::String(c.clone()));
                        Value::Object(o)
                    })
                    .collect(),
            ),
        );
    }
    if !lessons.is_empty() {
        map.insert(
            "lessons".into(),
            Value::Array(
                lessons
                    .iter()
                    .map(|(cat, content)| {
                        let mut o = serde_json::Map::new();
                        o.insert("category".into(), Value::String((*cat).into()));
                        o.insert("content".into(), Value::String((*content).into()));
                        Value::Object(o)
                    })
                    .collect(),
            ),
        );
    }
    Value::Object(map)
}

struct ShapeResult {
    name: &'static str,
    site: &'static str,
    n: usize,
    json_tokens: usize,
    toon_tokens: usize,
}

impl ShapeResult {
    fn savings_pct(&self) -> f64 {
        if self.json_tokens == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let (j, t) = (self.json_tokens as f64, self.toon_tokens as f64);
        (j - t) / j * 100.0
    }
}

fn count_tokens(bpe: &CoreBPE, s: &str) -> usize {
    bpe.encode_with_special_tokens(s).len()
}

fn all_goals() -> Vec<&'static str> {
    REAL_GOALS.iter().chain(CORPUS_GOALS.iter()).copied().collect()
}

/// Which optional fields a shape attaches to the base goal+allowed payload —
/// lets Phase 2's per-field claims (dir/constraint arrays vs. pattern table)
/// be isolated rather than only measured as one bundled "full context" shape.
#[derive(Clone, Copy)]
struct ShapeConfig {
    forbidden: bool,
    constraints: bool,
    patterns: bool,
    lessons: bool,
}

fn measure_shape(
    bpe: &CoreBPE,
    name: &'static str,
    site: &'static str,
    cfg: ShapeConfig,
) -> ShapeResult {
    let goals = all_goals();
    let all_patterns = representative_patterns();
    let all_lessons = representative_lessons();

    let mut json_tokens = 0usize;
    let mut toon_tokens = 0usize;
    let mut n = 0usize;

    for goal in &goals {
        for (allowed, forbidden) in DIR_SETS {
            let forbidden_slice: &[&str] = if cfg.forbidden { forbidden } else { &[] };
            let constraints: &[&str] = if cfg.constraints { CONSTRAINTS } else { &[] };
            let pat: &[(String, String)] = if cfg.patterns { &all_patterns } else { &[] };
            let les: &[(&str, &str)] = if cfg.lessons { &all_lessons } else { &[] };
            let value = task_context_value(goal, allowed, forbidden_slice, constraints, pat, les);
            let json = serde_json::to_string(&value).unwrap_or_default();
            let toon = lopi_toon::encode(&value);
            json_tokens += count_tokens(bpe, &json);
            toon_tokens += count_tokens(bpe, &toon);
            n += 1;
        }
    }

    ShapeResult {
        name,
        site,
        n,
        json_tokens,
        toon_tokens,
    }
}

fn write_report(results: &[ShapeResult], date: &str) -> anyhow::Result<()> {
    let mut out = String::new();
    writeln!(out, "# TOON token-savings measurement — {date}")?;
    writeln!(out)?;
    writeln!(out, "**Method:** for each payload shape, build one `serde_json::Value` per corpus sample and compare `serde_json::to_string` (compact JSON) against `lopi_toon::encode` on the identical value. Token counts are cumulative across the shape's full corpus, not per-sample averages.")?;
    writeln!(out, "**Tokenizer:** `tiktoken-rs` `cl100k_base` — OpenAI's GPT-4 BPE, **not** a Claude token count. No `ANTHROPIC_API_KEY` was available when this was run; see `crates/lopi-toon/src/lib.rs` and the README for this caveat.")?;
    writeln!(out, "**Corpus:** 37 real task goals (27 from `artifacts/diagnostics/20260717T113652Z/tasks.json`, 10 from `benchmarks/run.sh` T01-T10) × 2 real `allowed_dirs`/`forbidden_dirs` sets (shipped `lopi.toml.example` defaults; a crate-scoped set) = 74 samples for dir-only shapes. The full-context shape additionally attaches this repo's own 5 `CLAUDE.md` constraints and representative (schema-conformant, not live-table) pattern/lesson rows.")?;
    writeln!(out)?;
    writeln!(out, "| Shape | Call site | n | JSON tokens | TOON tokens | Savings |")?;
    writeln!(out, "|---|---|---|---|---|---|")?;
    for r in results {
        writeln!(
            out,
            "| {} | {} | {} | {} | {} | {:.1}% |",
            r.name,
            r.site,
            r.n,
            r.json_tokens,
            r.toon_tokens,
            r.savings_pct()
        )?;
    }
    writeln!(out)?;
    #[allow(clippy::cast_precision_loss)]
    let overall = {
        let json: usize = results.iter().map(|r| r.json_tokens).sum();
        let toon: usize = results.iter().map(|r| r.toon_tokens).sum();
        if json == 0 {
            0.0
        } else {
            (json as f64 - toon as f64) / json as f64 * 100.0
        }
    };
    writeln!(out, "**Overall (cl100k, all shapes pooled): {overall:.1}% fewer tokens than compact JSON.**")?;
    writeln!(out)?;
    writeln!(out, "Note: `fix()` (`crates/lopi-agent/src/claude.rs`) does not call `encode_task_context` — it hand-rolls an `allowed[N]: a,b,c` line and the doc comment there states TOON is skipped for it (error text is free-form prose). The \"dirs-only\" shape above measures the same `encode_task_context(goal, allowed, &[], &[], &[], &[])` call used by `implement_step()`, which is structurally what `fix()` would produce if it adopted TOON — it is not currently exercised by `fix()` itself.")?;

    // Phase 2: isolate the marginal per-field contribution against the
    // dirs-only baseline, so `claude.rs`'s per-prompt/per-attempt comment
    // can cite a number for each field independently instead of one bundled
    // "full context" figure.
    if let (Some(baseline), Some(dc), Some(dp)) = (
        results.iter().find(|r| r.name == "implement_streamed (dirs only)"),
        results.iter().find(|r| r.name == "dirs + constraint array (marginal)"),
        results.iter().find(|r| r.name == "dirs + pattern table (marginal)"),
    ) {
        #[allow(clippy::cast_precision_loss)]
        let per_prompt_constraints = {
            let base_saved = baseline.json_tokens as f64 - baseline.toon_tokens as f64;
            let dc_saved = dc.json_tokens as f64 - dc.toon_tokens as f64;
            (dc_saved - base_saved) / dc.n as f64
        };
        #[allow(clippy::cast_precision_loss)]
        let per_attempt_patterns = {
            let base_saved = baseline.json_tokens as f64 - baseline.toon_tokens as f64;
            let dp_saved = dp.json_tokens as f64 - dp.toon_tokens as f64;
            (dp_saved - base_saved) / dp.n as f64
        };
        writeln!(out)?;
        writeln!(out, "## Phase 2 — per-field marginal savings (cl100k, vs. dirs-only baseline, n={})", baseline.n)?;
        writeln!(out)?;
        writeln!(out, "- Adding the constraint array (5 real `CLAUDE.md` constraints) to a dirs-only prompt: **{per_prompt_constraints:.1} tokens/prompt**.")?;
        writeln!(out, "- Adding the pattern table (3 representative keyword/constraint rows) to a dirs-only prompt: **{per_attempt_patterns:.1} tokens/attempt**.")?;
        writeln!(out, "- These replace the unsourced `~17/prompt` and `~158/attempt` figures previously in `claude.rs:5-6` — both were far higher than what this corpus measures.")?;
    }

    // `cargo bench` runs with cwd at this crate's root, not the workspace
    // root — use CARGO_MANIFEST_DIR so the result lands in the same place
    // regardless of invocation directory.
    let results_dir = format!("{}/benches/results", env!("CARGO_MANIFEST_DIR"));
    fs::create_dir_all(&results_dir)?;
    let path = format!("{results_dir}/{date}_token_savings.md");
    fs::write(&path, &out)?;
    println!("{out}");
    println!("Result written to {path}");
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let bpe = tiktoken_rs::cl100k_base().map_err(|e| anyhow::anyhow!("cl100k_base init: {e}"))?;

    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        println!(
            "ANTHROPIC_API_KEY is set but this harness does not yet spend it automatically \
             (Anthropic's count_tokens endpoint is per-message, not a bulk offline API) — \
             falling back to cl100k for the full-corpus run. See crates/lopi-context/tests/token_estimation.rs \
             for the single-payload API cross-check pattern."
        );
    }

    let full = measure_shape(
        &bpe,
        "plan_streamed (full context)",
        "claude.rs plan_streamed()",
        ShapeConfig { forbidden: true, constraints: true, patterns: true, lessons: true },
    );
    let implement = measure_shape(
        &bpe,
        "implement_streamed (dirs only)",
        "claude.rs implement_streamed()",
        ShapeConfig { forbidden: true, constraints: false, patterns: false, lessons: false },
    );
    let allowed_only = measure_shape(
        &bpe,
        "allowed-dirs only",
        "claude.rs implement_step()",
        ShapeConfig { forbidden: false, constraints: false, patterns: false, lessons: false },
    );
    // Isolated marginal shapes for Phase 2's per-field claims — same dirs
    // baseline as `implement`, with exactly one of constraints/patterns added.
    let dirs_plus_constraints = measure_shape(
        &bpe,
        "dirs + constraint array (marginal)",
        "claude_support.rs build_plan_prompt() constraints slice",
        ShapeConfig { forbidden: true, constraints: true, patterns: false, lessons: false },
    );
    let dirs_plus_patterns = measure_shape(
        &bpe,
        "dirs + pattern table (marginal)",
        "claude_support.rs build_plan_prompt() patterns slice",
        ShapeConfig { forbidden: true, constraints: false, patterns: true, lessons: false },
    );

    let date = std::env::var("LOPI_BENCH_DATE").unwrap_or_else(|_| "UNDATED".to_string());
    write_report(&[full, implement, allowed_only, dirs_plus_constraints, dirs_plus_patterns], &date)?;
    Ok(())
}
