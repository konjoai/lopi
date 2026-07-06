# Next — Skill Arguments (Capability 4)

Per `PROMPTS_PLAN.md`'s sprint order, Capability 4 (Skill Arguments) comes
next and must **reuse** this sprint's substitution primitive rather than
writing a second one (same DRY constraint the Konjo quality gate enforces at
>10 lines / >85% similarity).

## What Sprint 1 built, for the next sprint to build on

```rust
// crates/lopi-core/src/template.rs
pub fn resolve(
    template: &str,
    vars: &std::collections::BTreeMap<String, String>,
) -> Result<String, TemplateError>;

pub enum TemplateError {
    UnresolvedVariable { name: String, template: String },
}

// crates/lopi-core/src/task.rs
impl Task {
    pub fn from_template(
        template: &str,
        vars: &std::collections::BTreeMap<String, String>,
    ) -> Result<Self, crate::template::TemplateError>;
}
```

Also re-exported at the crate root as `lopi_core::{resolve_template, TemplateError}`
(renamed on export, mirroring the existing `schema::validate` → `validate_schema`
convention, to keep a generic name off the crate root).

## How Capability 4 should use it

`PROMPTS_PLAN.md`'s plan for skill arguments was:

- `Skill::render_body(&self, args: &str) -> String` — replace the literal
  `$ARGUMENTS` placeholder in `Skill::body` with `args`. **Implement this by
  calling `template::resolve`**, not a second `.replace()` — the cleanest way
  is to treat `$ARGUMENTS` as a single well-known hole. Two options to weigh
  in that sprint:
  1. Translate `$ARGUMENTS` to `{arguments}` before calling `resolve`, with a
     one-entry vars map `{"arguments": args}` — zero changes to `template.rs`.
  2. Extend `resolve` to also recognize a `$NAME`-style hole — only justified
     if skill bodies need more than one substitutable value.
  Start with (1); it's strictly smaller and doesn't touch `template.rs` at all.
- A minimal `:<skill-name> <rest>` prefix parser at the goal-ingestion
  boundary (Telegram `handlers.rs`, CLI) that looks up the skill by name and
  passes `rest` as `args`.

## Constraint carried forward

No frontmatter/schema change for Capability 4 — `$ARGUMENTS` lives in the
skill body markdown (already a `String`), so `Skill` needs no new field. See
`LEDGER.md`'s Sprint 1 entry before touching template escaping semantics.
