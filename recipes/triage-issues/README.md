# triage-issues

## ⚠ This recipe processes untrusted input

Issue text comes from anyone who can file an issue against the repo — that
includes attackers. Read this entire README, especially "Known containment
gap" below, before applying this recipe. Nothing here should ever be
combined with an autonomy level above `report_only`, and it must never be
given outbound network or comms tools.

## What it does

Reads one incoming issue, assigns a triage label (bug/feature/question/
duplicate), and writes a one-paragraph summary — output only, never an
action taken on the repo or an external system.

## F0 rationale

The simpler thing this beats is a human reading every incoming issue and
labeling it themselves — the right call for low volume, and the only
acceptable call if you can't accept the residual risk described below. This
recipe earns its keep only where issue *volume* makes a first-pass triage
genuinely useful, and only because its blast radius is capped at "wrong
label, wrong summary" — never "the untrusted issue text got to act on
anything." If you can't make that capping true in your environment (see
below), you have not earned this loop yet, however high the volume.

## Principles demonstrated

- **F3 — three hard stops**, explicit and deliberately small:
  `max_iterations = 4`, `no_progress_limit = 2`, `[budget] preset = "quick"`.
- **F10 — least privilege, contain by construction.** This is the recipe
  the whole legend entry is named for. See "Known containment gap" — the
  honest version of what's actually enforced today.
- **Bonus: F2.** `verifier_required = true` — a second pass reviews the
  triage output itself, on the theory that summarizing adversarial text is
  exactly the case that benefits from an independent second read before the
  output is trusted.
- **No F1.** Deliberately: there is no deterministic oracle for "is this
  summary good" the way there is for a failing test. That absence is itself
  part of why this recipe stays at `report_only` — without a checker to
  lean on, a human reviewing every output is the only remaining safeguard.

## Known containment gap — read before applying

This sprint's pre-flight kill-test (required by this repo's own process —
see `recipes/README.md`) traced `permission_deny`/`permission_allow` all the
way from `.lopi/loop.toml` to the `claude` CLI invocation, rather than
assuming the field's own doc comment was still accurate. It isn't, for the
path this recipe uses:

- **What's actually wired**: `crates/lopi-orchestrator/src/pool/run_loop.rs`
  and `src/run_command.rs` both build the runner's `--allowedTools`/
  `--disallowedTools` from `LoopConfig::resolved_budget()` — i.e. purely
  from `[budget]`'s preset (`quick`/`standard` deny `Workflow`/`Task`/
  `Agent`; `deep`/`unlimited` deny nothing) plus `[budget].permission_allow`.
- **What's *not* wired**: the flat, top-level `permission_deny`/
  `permission_allow` fields (the ones this recipe sets to
  `["Workflow", "Task", "Agent", "WebFetch", "WebSearch"]`) are read by
  exactly one code path in this repo: `lopi loop show`'s display
  (`src/loop_commands.rs`). They have **no effect on the actual runner** —
  confirmed by reading both call sites, not inferred from the doc comment.
- **The consequence**: as shipped today, this recipe's `permission_deny`
  does **not** block `WebFetch`/`WebSearch` at runtime. The `quick` preset's
  own deny list *does* still block `Workflow`/`Task`/`Agent` (sub-agent
  fan-out) — that part is real. Everything else in `permission_deny` here
  is a statement of intent, not an enforced boundary, until that gap is
  closed (tracked in this sprint's `NEXT_SESSION_PROMPT.md`).
- **What actually contains the blast radius today**: `autonomy_level =
  "report_only"`. Whatever a compromised triage run does or doesn't touch,
  it cannot open a PR, merge anything, or reach a Telegram/WhatsApp send —
  those all require a higher autonomy level and (for the latter) an
  explicit `egress_allowed_chat_ids` entry this recipe never grants. Treat
  `report_only` as load-bearing, not a formality, for this specific recipe.

**Until the gap above is closed, do not raise this recipe's autonomy level,
and do not rely on `permission_deny` to keep an untrusted-input loop off
the network** — review its output before acting on it, the same as you
would a summary a stranger handed you.

## Stop conditions

| Field | Value | Why |
|---|---|---|
| `max_iterations` | `4` | Triage is one read-and-summarize pass, not an iterative fix loop — small on purpose. |
| `no_progress_limit` | `2` | Repeated malformed output after 2 attempts means something's wrong with the input or the prompt, not that one more try will help. |
| `[budget] preset = "quick"` | $1 / 200K tokens, fan-out denied | The one containment control in `[budget]` that *is* real (see above) — keep it on `quick`/`standard`, never `deep`/`unlimited`, for this recipe. |

## Expected cost and duration

**TODO — pending live-run measurement.** Will be filled in from a real
`lopi_submit_task` run against a scratch repo containing one issue with an
embedded prompt-injection attempt, per `recipes/README.md`'s
applied-and-run steps, before this sprint closes — including confirmation
that the injected instructions were not followed.

## When not to use this

- **You need this to eventually auto-label/auto-comment on GitHub itself.**
  This recipe produces a local report only; wiring it to actually post back
  to GitHub is a larger trust decision this recipe deliberately doesn't make.
- **Your environment can't accept the containment gap above.** If you need
  `WebFetch`/`WebSearch` genuinely denied at runtime today, don't apply this
  recipe until that's fixed — running it anyway and trusting the config to
  protect you is worse than not automating this at all.
- **The issue queue is not adversarial** (an internal-only tracker with no
  public submission path). This recipe's caution is priced for the
  adversarial case; a fully trusted queue may reasonably want a lighter loop.
