# The Honest Measurement Policy

This is a public commitment, not an internal note.

> Every number lopi displays states what it measures and where it came from.
> lopi reports what it can observe directly. Where a number is unavailable,
> lopi says it is unavailable and why, rather than estimating without saying
> so or obtaining it through means the source did not intend.

## Three prohibitions

lopi will not do the following to make a number look nicer, ever:

1. **No bypassing bot protection.** If a usage endpoint sits behind a
   challenge, lopi does not replay clearance tokens, spoof fingerprints, or
   impersonate a browser to get past it.
2. **No undocumented internal APIs** as a data source for a user-facing
   number.
3. **No stored third-party session credentials** for the purpose of reading
   a nicer statistic.

The correct response to an unavailable number is to not show it, and to
explain the gap — not to estimate silently, and not to reach for a means the
source didn't intend.

## Provenance

Every user-facing metric carries a `Provenance` value
(`lopi_core::Provenance`, `crates/lopi-core/src/provenance.rs`) — attaching
it is enforced by the type system, not convention: a metric-carrying struct
without a `Provenance` field doesn't compile against the rendering paths
that expect one.

```rust
pub enum Provenance {
    /// lopi counted this itself from its own runs.
    Measured { source: &'static str },
    /// A tool or API reported it; lopi passes it through.
    Reported { source: &'static str, as_of: DateTime<Utc> },
    /// Derived from measured values plus assumptions.
    Estimated { basis: &'static str, as_of: DateTime<Utc> },
    /// Known to exist, not obtainable honestly.
    Unavailable { reason: &'static str },
}
```

Every rendering path — TUI, web, MCP widget, CLI stdout — renders the
provenance **next to the number**, not in a tooltip and not only in these
docs.

**Naming note:** `GET /api/tasks` already serializes a field named
`"provenance"` with a different meaning — `TaskRow::provenance()` is a
*trust* classification (did this task come from an authenticated operator
path or an unauthenticated webhook?), unrelated to measurement confidence.
To avoid colliding with it, every JSON field carrying a `Provenance` value
from this policy is named `"measurement_provenance"`.

## `/cost` and every other cost/usage surface

As of this sprint, lopi's Telegram bot (and its `/cost` command) no longer
exists — it was removed in Sprint S10, Phase 4 (see `docs/adr/0001-demo-mode-and-measurement.md`
for how this sprint discovered and adapted to that). The same treatment this
policy would have applied to `/cost` now applies to the surfaces that
actually carry a cost/token figure today:

- `GET /api/stats` (`total_cost_usd_today`, `total_tokens_today`)
- `GET /api/budget/breakdown`
- `lopi run` / the loop runner's `"💵 session cost"` stdout line
- The REPL's session-cost status line

Each states, in plain language, next to the number:

- **What it is**: local token burn, counted by lopi from its own agent
  runs (`turn_metrics` rows this process wrote).
- **What it is not**: not your Anthropic plan quota, not account usage, not
  a bill, and not inclusive of any Claude Code session you ran by hand
  outside lopi.
- If lopi cannot see plan quota (it can't — no documented API exists for
  that on a subscription account, and building one would mean bypassing bot
  protection or reverse-engineering an undocumented endpoint, both
  prohibited above), the surface says so in one line rather than leaving
  you to assume the number is your remaining allowance. That assumption is
  the whole problem this policy exists to prevent.

## The dollar-figure trap

Any time lopi converts a token count into a currency estimate, it is doing
arithmetic against a price table that can go stale silently — the single
most likely way lopi shows a confidently wrong number.

- The price table lives in **one place**: `crates/lopi-agent/pricing.toml`,
  versioned, with an explicit top-level `as_of` date.
- The `as_of` date renders with every dollar estimate that uses it.
- `crates/lopi-agent/src/pricing.rs::is_stale()` flags the table once it's
  older than `STALENESS_THRESHOLD_DAYS` (90 days) — at that point, a caller
  degrades the estimate to a warning (`staleness_warning()`) instead of a
  confident figure.
- Every dollar estimate derived from `pricing.toml` is classified
  `Provenance::Estimated`, never `Measured`. The one exception: the `claude`
  CLI's own authoritative `result.total_cost_usd`, which lopi passes through
  unmodified — that's `Provenance::Reported`, since lopi isn't the one doing
  the arithmetic.
- Currency estimates are the one surface this sprint leaves opt-out rather
  than opt-in by default (unchanged from before this sprint) — tokens are
  measured directly; dollars are always a model on top of them. A future
  sprint may revisit making them explicitly opt-in.

## Demo mode and provenance

`lopi demo` (`docs/adr/0001-demo-mode-and-measurement.md`) fabricates a
complete synthetic store. Every metric it produces is real Rust data of the
same shape a live run would produce — the number itself isn't fake in the
sense of being malformed — but it did not come from an actual agent run, so
presenting it with `Provenance::Measured` would be a lie by omission. Demo
mode is a `Provenance` case of its own: a value from a store where
`MemoryStore::is_synthetic()` is `true` renders with a `synthetic: true`
marker alongside (or instead of) its usual provenance label, on every
surface — so a screenshot of a demo dashboard cannot be mistaken for a real
benchmark, including by us, by accident, later.

Demo mode is also the best place to see `Provenance::Unavailable` — the
variant hardest to notice in a screenshot of a real run, because a real run
mostly has all its "available" numbers filled in. The generator seeds at
least one metric through every provenance variant on purpose.

## Known gaps

A full-workspace grep for dollar/token/usage figures found real numbers
rendered without a `Provenance` label in places this sprint did not reach.
Each is tracked here rather than left silently unlabeled, per this repo's
`KNOWN DEBT` convention (`.konjo/scripts/soft_gate_lint.py`):

- **`web/src/` (the SvelteKit dashboard)** — roughly 20 files independently
  format cost/token figures (`web/src/routes/budget/+page.svelte`,
  `web/src/routes/loop/+page.svelte`, `web/src/lib/stores/budget.ts`,
  `web/src/lib/components/stacks/RunStatsPill.svelte`, and others) with no
  shared `Provenance`-aware formatting helper and no visible label. A
  parallel TypeScript `Provenance` type and a shared badge component are
  needed to bring the frontend to parity with the Rust API's
  `measurement_provenance` fields — out of scope for this sprint.
- **`GET /api/loop-engineering/health`, `GET /api/loop-engineering/runs`** —
  per-turn and per-run `cost_usd` fields (`crates/lopi-ui/src/web/loop_health_handlers.rs`,
  `loop_runs_handlers.rs`) are unlabeled. Lower traffic than `/api/stats`
  and `/api/budget/breakdown`, which this sprint did label.
  KNOWN DEBT, verified 2026-07-28.
- **`crates/lopi-ratelimit/src/circuit_breaker.rs`**'s
  `"hourly cost cap exceeded: ${cap:.2}/hr"` error string is a raw dollar
  figure with no provenance context (it's derived from the same
  `pricing.toml`-based estimate as everything else `Estimated`, but the
  error type carries no marker saying so). KNOWN DEBT, verified 2026-07-28.
- **`crates/lopi-ui/src/tui/cognition.rs`**'s cost/token panel state
  (`TurnMetricsSample`, `Cost` event sample) has no provenance rendering yet
  in `draw.rs` — the TUI's synthetic-store badge (this sprint) landed on the
  header bar, not on this panel specifically. KNOWN DEBT, verified
  2026-07-28.

Each of these is a real number, correctly computed — the debt is the
missing label, not the value.
