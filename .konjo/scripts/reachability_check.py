#!/usr/bin/env python3
"""Konjo advisory reachability check — Sprint F0.

`RUSTFLAGS="-W dead_code"` (the G1 dead-code gate, konjo-gate.yml) certifies
that every `pub mod` is *rustc*-reachable: a `pub` item in a library crate
counts as public API and is never flagged dead, regardless of whether the
shipped binary (`src/`) ever calls into it. That is a different property
from "a user running the built `lopi` binary can reach this code" — Sprint
F0 found `lopi-remote::whatsapp` passing the dead-code gate while being
completely unreachable from `src/` (see CHANGELOG.md and
docs/security/TRIFECTA_PATHS.md).

This is a single-binary workspace (`crates/*` are all internal library
crates of the `lopi` application, not a mix of externally-consumed
packages), so "reachable from the binary" is approximated as "referenced
anywhere in the workspace outside the crate that defines it" — the binary
(`src/`) reaches most workspace code transitively (`src/` calls
`lopi-orchestrator`, which calls `lopi-agent`, which calls `lopi-core`,
etc.), not only through direct references from `src/` itself. Restricting
the search to literal text under `src/` was tried first and flagged ~40
modules that are genuinely used, just one or more hops away from `src/` —
that is noise that buries the real signal, not a finding. A prior version
of this script did exactly that; see git history if you need the
comparison.

For every top-level `pub mod <name>;` in a workspace crate's `src/lib.rs`:

  - If `lib.rs` re-exports the module at the crate root (`pub use <name>::
    {A, B};` or `pub use <name>::A;`) — the normal pattern in this
    workspace — reachability is checked against those re-exported item
    names: is `A` or `B` referenced anywhere in the rest of the workspace?
    A facade crate's modules are meant to be consumed through the
    re-export, not the qualified path, so checking the qualified path here
    (as a naive check would) flags nearly every module in every facade
    crate.
  - Otherwise (no re-export — the caller has no way to reach it except the
    qualified path, exactly `lopi-remote`'s shape, which has no `pub use`
    at all) — reachability is checked against the qualified path
    `<crate_ident>::<name>` appearing anywhere in the rest of the
    workspace.

"Anywhere in the rest of the workspace" excludes the defining crate's own
`src/` (a module always "reaches" itself trivially) but includes every
other crate's `src/` plus the root `src/` — a module used only by its own
crate's other modules, and never referenced by anything else in the
workspace, is exactly the shape that turned out to indicate genuine
dead-from-the-binary code for `lopi-remote::whatsapp` (verified by hand:
it is not referenced in `lopi-ui`, `lopi-core`, or anywhere else — the
"whatsapp" string that *does* appear in those crates is an unrelated
config/report-channel label, not a call into this module).

This is intentionally advisory — a `pub mod` can be legitimate library
surface for external consumers of the crate (published to crates.io, used
by another workspace crate, exercised only in tests) and unreached-by-the-
binary is not on its own a defect. The point is visibility: the next module
that quietly becomes unreachable should show up here before it also lingers
in the README, not after.

It is still a grep-shaped heuristic, not a real reachability analyzer: no
macro expansion, no trait-object indirection, no cross-crate re-export
chains beyond one level. Read the flagged list; don't just count it.

Exit code is always 0 — see the module docstring above and the
`ADVISORY BY DESIGN` marker on this step in konjo-gate.yml. This never
blocks a merge.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
PUB_MOD_RE = re.compile(r"^\s*pub mod\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*;")
PACKAGE_NAME_RE = re.compile(r'^\s*name\s*=\s*"([^"]+)"', re.MULTILINE)
# Matches `pub use <mod>::{A, B, c as D};` or `pub use <mod>::Item;`,
# tolerating the multi-line brace form this workspace's lib.rs files use.
PUB_USE_RE = re.compile(r"pub use\s+([a-zA-Z_][a-zA-Z0-9_]*)::(\{[^}]*\}|[a-zA-Z_][a-zA-Z0-9_]*)\s*;", re.DOTALL)
IDENT_RE = re.compile(r"[a-zA-Z_][a-zA-Z0-9_]*")


def crate_ident(cargo_toml: Path) -> str | None:
    """Extract the crate's `[package] name`, as a Rust path identifier
    (hyphens become underscores, matching how `use crate_name::mod` is
    actually written)."""
    text = cargo_toml.read_text(encoding="utf-8")
    m = PACKAGE_NAME_RE.search(text)
    return m.group(1).replace("-", "_") if m else None


def public_modules(lib_text: str) -> list[str]:
    return [m.group(1) for line in lib_text.splitlines() if (m := PUB_MOD_RE.match(line))]


def reexported_items(lib_text: str) -> dict[str, list[str]]:
    """Map module name -> list of item idents it re-exports at the crate root."""
    out: dict[str, list[str]] = {}
    for m in PUB_USE_RE.finditer(lib_text):
        mod, body = m.group(1), m.group(2)
        idents = IDENT_RE.findall(body) if body.startswith("{") else [body]
        # Drop `as` aliases' left-hand original name isn't needed here —
        # both sides are valid idents a caller could reference, so keep all.
        out.setdefault(mod, []).extend(idents)
    return out


def read_all_rs(*dirs: Path) -> str:
    return "\n".join(
        p.read_text(encoding="utf-8", errors="replace")
        for d in dirs
        if d.is_dir()
        for p in d.rglob("*.rs")
    )


def workspace_text_excluding(own_crate_dir: Path) -> str:
    """Every `.rs` file in the workspace except `own_crate_dir`'s own
    `src/` — see module docstring for why self-references don't count."""
    parts = [read_all_rs(REPO_ROOT / "src")]
    for cargo_toml in sorted((REPO_ROOT / "crates").glob("*/Cargo.toml")):
        crate_dir = cargo_toml.parent
        if crate_dir == own_crate_dir:
            continue
        parts.append(read_all_rs(crate_dir / "src"))
    return "\n".join(parts)


def word_present(text: str, word: str) -> bool:
    return re.search(rf"\b{re.escape(word)}\b", text) is not None


def main() -> int:
    unreached: list[str] = []

    for cargo_toml in sorted((REPO_ROOT / "crates").glob("*/Cargo.toml")):
        crate_dir = cargo_toml.parent
        lib_rs = crate_dir / "src" / "lib.rs"
        if not lib_rs.is_file():
            continue
        ident = crate_ident(cargo_toml)
        if ident is None:
            continue

        lib_text = lib_rs.read_text(encoding="utf-8")
        reexports = reexported_items(lib_text)
        rest_of_workspace = workspace_text_excluding(crate_dir)

        for mod in public_modules(lib_text):
            items = reexports.get(mod)
            if items:
                if not any(word_present(rest_of_workspace, item) for item in items):
                    unreached.append(
                        f"{crate_dir.name}::{mod}  (re-exports {items}; none referenced elsewhere in the workspace)"
                    )
            else:
                needle = f"{ident}::{mod}"
                if needle not in rest_of_workspace:
                    unreached.append(
                        f"{crate_dir.name}::{mod}  (not re-exported; `{needle}` not referenced elsewhere in the workspace)"
                    )

    print("Reachability check (advisory — pub mod unreached from the binary):")
    if unreached:
        for line in unreached:
            print(f"  - {line}")
        print(
            f"\n{len(unreached)} module(s) flagged. This is advisory, not a failure — "
            "see .konjo/scripts/reachability_check.py's docstring for why."
        )
    else:
        print("  (none — every pub mod in a workspace crate is reachable from src/)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
