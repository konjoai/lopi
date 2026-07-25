---
name: konjo-ship
description: Konjo sprint completion checklist and session handoff template. Use when closing out a sprint or ending a work session, in any consuming repo.
user-invocable: true
---
# Konjo Ship

## Self-update preamble (run first)

```bash
bash "$HOME/.konjo/kiban/plugins/konjo/hooks/preamble_update.sh"
```

## Sprint Completion Checklist

A sprint is not complete until every one of these is true:

```
[ ] All success criteria met
[ ] All tests pass — the repo's `verify_cmd` green, zero failures
[ ] Repo lint/format gate clean (cargo clippy, ruff, eslint — whatever this repo runs)
[ ] CHANGELOG.md updated — human-readable, what changed and why it matters
[ ] LEDGER.md updated for any one-way door this sprint crossed
[ ] `konjo-doc-staleness scan` run repo-wide. A FAIL on a doc this sprint relates to
    blocks shipping — re-stamp, rewrite, or reclassify `historical`. A FAIL unrelated
    to this sprint is reported in the handoff below, not a blocker (a small task
    should not get stuck behind someone else's pre-existing debt).
[ ] No doc *this sprint touched* asserts a capability state that contradicts the code
[ ] Zero debug artifacts, dead code, or leftover scaffolding
[ ] git add && git commit -m "type(scope): description" && git push
```

A sprint that is "basically done" is not done. Ship clean or don't ship.

No item above names a specific file — a checklist that enumerates filenames catches
drift in those files and nothing else. Pre-existing staleness elsewhere is surfaced
every session, so it is never silently forgotten, without taxing unrelated work.

## Execute Checklist

*ᨀᨚᨐᨚ — Build the ship. Make it seaworthy.*

```
PLAN    — write the implementation steps before touching code
BUILD   — one step at a time, logical commits
TEST    — run existing tests, write new ones, fix failures immediately
REVIEW  — re-read everything just written — is it beautiful? is it lean? is it Konjo?
ITERATE — when something breaks, go back to the source — no papering over
SHIP    — all tests pass, docs updated, changelog written, then push
```

When things break — apply *根性*:
- **Test fails** — analyze the stack trace at root. State the flaw precisely. Fix it. No apologies.
- **Benchmark looks wrong** — investigate the measurement before concluding the approach is wrong.
- **Architecture isn't working** — find another angle. Search the literature. Invent if necessary.

## Session Handoff Template

```
SHIPPED      [what was completed this session]
TESTS        [passing / failing / count]
PUSHED       [commit hash or "not pushed — reason"]
NEXT SESSION [the exact next task — not "continue the work"]
DOC DEBT     [decays: state FAILs unrelated to this sprint, found but not fixed here]
DISCOVERIES  [papers, repos, techniques found this session worth revisiting]
HEALTH       [Green / Yellow / Red — one line]
```

Every session is a step toward something larger. Make the handoff count.
*Mahiberawi Nuro — we build together. Leave the work ready for the next person.*

## Per-repo override

This skill ships from the global clone, not copied per repo, so there is one file to
keep current instead of one per consuming repo. A repo that genuinely needs a
different checklist keeps a repo-scoped `.claude/skills/konjo-ship/SKILL.md`: Claude
Code resolves the repo-scoped skill over the identically-named global one, so the
local copy wins there with no special-casing here. Prefer changing this file for
anything that should apply everywhere; reach for an override only for a real
divergence, and say why in the override file so the next session finds the reason.
