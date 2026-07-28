//! Static content pools for `lopi demo`'s fixture generator.
//!
//! Every string here is invented — no real repo, org, person, or company —
//! reviewed by hand per the sprint's content-hygiene requirement (see
//! `docs/adr/0001-demo-mode-and-measurement.md`). [`generator`](super::generator)
//! picks deterministically from these pools using a seeded RNG; this module
//! holds no randomness itself.

/// One synthetic repo the demo store fabricates traffic against. Paths are
/// obviously synthetic (`/demo/repos/...`) — never a real home directory
/// shape — so nobody mistakes one for a path that exists on their machine.
pub struct RepoTemplate {
    /// Fictional repo name, used as its `demo_repos` primary key.
    pub name: &'static str,
    /// Short stack label shown in the dashboard's repo picker.
    pub stack: &'static str,
    /// Synthetic filesystem path — never scanned, never opened.
    pub path: &'static str,
    /// One-line description.
    pub description: &'static str,
}

/// The four synthetic repos every demo store ships, regardless of seed —
/// only the tasks/traffic generated against them vary by seed. Fixed
/// identity keeps `--seed`-driven screenshots comparing apples to apples
/// across runs.
pub const REPOS: [RepoTemplate; 4] = [
    RepoTemplate {
        name: "aurora-api",
        stack: "Rust service",
        path: "/demo/repos/aurora-api",
        description: "Order-processing API — axum + sqlx, deployed as a fleet of stateless workers.",
    },
    RepoTemplate {
        name: "lumen-console",
        stack: "TypeScript web app",
        path: "/demo/repos/lumen-console",
        description: "Internal admin console — SvelteKit front end over the aurora-api.",
    },
    RepoTemplate {
        name: "tidepool-etl",
        stack: "Python data pipeline",
        path: "/demo/repos/tidepool-etl",
        description: "Nightly ETL jobs — ingests order events, writes the analytics warehouse.",
    },
    RepoTemplate {
        name: "wayfarer-infra",
        stack: "Infra (Terraform)",
        path: "/demo/repos/wayfarer-infra",
        description: "Terraform + Helm charts for the staging and production clusters.",
    },
];

/// A candidate goal for one repo, keyed by the repo's index into [`REPOS`].
pub struct GoalTemplate {
    /// Index into [`REPOS`].
    pub repo_index: usize,
    /// The goal text itself — reads like real engineering work, not lorem ipsum.
    pub goal: &'static str,
}

/// Goal pool — plausible engineering tasks across all four repos. More
/// entries than any single seed needs, so the generator can pick a
/// deterministic subset per seed and still vary which goals appear.
pub const GOALS: &[GoalTemplate] = &[
    // aurora-api (0) — Rust service
    GoalTemplate { repo_index: 0, goal: "fix the flaky retry test in the checkout handler" },
    GoalTemplate { repo_index: 0, goal: "add rate limiting to the /webhooks ingress endpoint" },
    GoalTemplate { repo_index: 0, goal: "migrate the audit_log table to a JSONB payload column" },
    GoalTemplate { repo_index: 0, goal: "fix the connection pool leak under sustained load" },
    GoalTemplate { repo_index: 0, goal: "add idempotency keys to the /orders POST endpoint" },
    GoalTemplate { repo_index: 0, goal: "reduce p99 latency on the inventory lookup query" },
    GoalTemplate { repo_index: 0, goal: "backfill missing tracing spans in the payment module" },
    GoalTemplate { repo_index: 0, goal: "fix the panic on malformed webhook signatures" },
    // lumen-console (1) — TypeScript web app
    GoalTemplate { repo_index: 1, goal: "fix the stale cache bug on the orders table filter" },
    GoalTemplate { repo_index: 1, goal: "backfill missing type exports in the SDK client package" },
    GoalTemplate { repo_index: 1, goal: "add keyboard navigation to the bulk-actions toolbar" },
    GoalTemplate { repo_index: 1, goal: "fix the layout shift on the dashboard's first paint" },
    GoalTemplate { repo_index: 1, goal: "migrate the legacy REST calls to the generated client" },
    GoalTemplate { repo_index: 1, goal: "add a loading skeleton to the customer detail panel" },
    GoalTemplate { repo_index: 1, goal: "fix the timezone bug in the audit trail timestamps" },
    // tidepool-etl (2) — Python data pipeline
    GoalTemplate { repo_index: 2, goal: "reduce the nightly ETL job's memory footprint under 2GB" },
    GoalTemplate { repo_index: 2, goal: "fix the duplicate-row bug in the order-events dedup step" },
    GoalTemplate { repo_index: 2, goal: "add a dead-letter queue for malformed ingest records" },
    GoalTemplate { repo_index: 2, goal: "speed up the warehouse backfill script past the 6h SLA" },
    GoalTemplate { repo_index: 2, goal: "fix the schema drift check against the analytics warehouse" },
    GoalTemplate { repo_index: 2, goal: "add retry-with-backoff to the upstream API fetch step" },
    // wayfarer-infra (3) — Infra
    GoalTemplate { repo_index: 3, goal: "add a health-check probe timeout to the ingress controller" },
    GoalTemplate { repo_index: 3, goal: "fix the Terraform drift on the staging security groups" },
    GoalTemplate { repo_index: 3, goal: "add autoscaling limits to the aurora-api worker pool" },
    GoalTemplate { repo_index: 3, goal: "rotate the staging cluster's expiring TLS certificates" },
    GoalTemplate { repo_index: 3, goal: "fix the Helm chart values that silently ignore --set overrides" },
];

/// Log-line templates for the demo's task_logs traffic. `{stage}` is
/// substituted by the generator with the pipeline stage name.
pub const LOG_LINE_TEMPLATES: &[&str] = &[
    "starting {stage}",
    "{stage}: reading repo context",
    "{stage}: applying constraints from prior lessons",
    "{stage}: {stage} complete",
    "running test suite",
    "lint pass: 0 errors",
    "diff size: within budget",
    "score: composite quality above threshold",
];

/// Pattern content pool — `(goal_keywords, successful_constraints)` pairs
/// that read like something actually mined from repeated runs.
pub const PATTERNS: &[(&str, &str)] = &[
    ("flaky retry test", "add explicit backoff jitter and re-run the suite 3x before scoring"),
    ("rate limiting webhook", "reuse the existing token-bucket middleware rather than a bespoke limiter"),
    ("memory footprint etl", "stream batches instead of loading the full frame into memory"),
    ("terraform drift", "run `terraform plan` before touching any `.tf` file, never trust cached state"),
    ("type exports sdk", "regenerate from the OpenAPI spec instead of hand-editing generated types"),
    ("timezone bug", "store and compare timestamps in UTC, format at the display boundary only"),
];

/// Lesson content pool — `(category, content)` pairs.
pub const LESSONS: &[(&str, &str)] = &[
    ("strategy", "When a goal mentions 'flaky', reproduce the failure locally before proposing a fix — most flakes are ordering bugs, not the symptom described."),
    ("recovery", "A rolled-back attempt on a migration task usually means the down-migration was missing, not that the up-migration was wrong — check both before retrying."),
    ("optimization", "Batching writes inside one transaction cut attempt wall-clock by more than any code-level micro-optimization tried first."),
    ("strategy", "Infra goals that mention 'drift' benefit from a dry-run diff attached to the PR description, not just a summary sentence."),
    ("recovery", "A conflict on a shared config file is usually resolved by rebasing onto the latest default branch before retrying, not by force-pushing over it."),
];

/// Dead-letter blocker descriptions — plausible reasons a goal genuinely
/// couldn't be completed, used on the demo's one honest-failure story.
pub const BLOCKERS: &[&str] = &[
    "exhausted max_retries (3) — every attempt hit the same pre-existing test failure on main, unrelated to the goal",
    "diff scope violation — the minimal fix touched a forbidden directory (infra/) and no in-scope alternative was found",
    "budget exceeded — the task's token budget was consumed across retries without reaching a passing score",
];
