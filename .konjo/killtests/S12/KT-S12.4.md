# KT-S12.4 — Swift persistence and input-surface inventory (not pass/fail)

**Sprint:** S12, Phase 4 · **Type:** inventory, per the sprint brief's own framing. Full table
lives in `docs/security/TRIFECTA_PATHS.md` §8 ("Sprint S12, Phase 4 — Swift review").

## Summary

Six areas enumerated across `macos/` + `packages/LopiStacksKit/` (~19k LOC): Keychain usage
beyond `ServerConfig`, URL/deep-link handling, HTML/web-view rendering of agent output, App
Transport Security exceptions, entitlements, and unencrypted disk writes. **All six came back
clean** — no fixable finding in any of them:

- Keychain usage is correctly scoped to the one thing that needs it (the bearer token,
  `ServerConfig.swift:47-85`); nothing else persists a credential to `UserDefaults` or disk.
- No custom URL scheme or universal link exists at all — zero deep-link attack surface.
- Agent-produced text (task logs, plans, diffs) renders via SwiftUI's native
  `AttributedString(markdown:)`, not a web view or HTML-mode attributed string — not the
  stored-XSS shape S11 Phase 2 found on the web side.
- No ATS exceptions declared; default (strict) ATS applies.
- Entitlements are sandboxed and minimal — network client only, no file-access grants beyond
  what the app actually uses.
- No sensitive content (tokens, transcripts, logs) is ever written to disk outside Keychain;
  live state is held in memory only.

One documentation-only note recorded (not a fix, since the reachable case is the one ATS
already appears to block): `ServerConfig.swift`'s `baseURL`/`webSocketURL` are hardcoded
`http://`/`ws://`, fine against the default loopback host but worth flagging in a comment for
whoever next touches `host` — added this sprint (`macos/Lopi/Store/ServerConfig.swift`).

## Why this is a kill-test file and not just "nothing to report"

Per the sprint brief: "Inventory first, fixes second." An inventory that finds nothing is still
the deliverable — the alternative to running it is *assuming* it's clean, which is exactly what
this file exists to replace with an actual, cited check. See `docs/security/TRIFECTA_PATHS.md`
§8 for the full per-area detail and file:line citations.
