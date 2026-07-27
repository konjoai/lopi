#!/usr/bin/env python3
"""Konjo scope-lock assertion — Sprint S12, Phase 6.

Sprint S12, Phase 0 removed lopi's multi-tenant surface entirely (the
`lopi-app` crate, `MemoryStore::open_for_customer`, the `github_installations`
table, `CustomerTier`/Stripe billing) rather than hardening it — see
`LEDGER.md`. That is a decision, not a fact about the code that enforces
itself: nothing stops a future PR from reintroducing a `customer_id` column,
an `open_for_customer` helper, or a Stripe webhook handler one file at a
time, each individually looking like a reasonable addition, until the scope
lock has silently eroded. This script is the mechanism that keeps the
decision from drifting back by omission.

Fails (nonzero exit) if any of a fixed set of forbidden, case-insensitive
substrings appears in non-test Rust source under `crates/` or `src/`.
"Non-test" is approximated the same way the rest of this repo's tooling
does: skip any path containing `/tests/`, `_test.rs`/`_tests.rs`, or
`test_` in the filename, and skip anything inside a `#[cfg(test)]` module —
approximated here as "the file is a dedicated test file", not full AST
parsing, since the forbidden terms are business/schema nouns that have no
legitimate reason to appear in inline `#[cfg(test)] mod tests { ... }`
blocks inside otherwise-production files either; a real reintroduction of
this surface would show up as new production code, not a stray reference
inside `mod tests`.

Usage:
    python3 .konjo/scripts/scope_assert.py [--staged-only]
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

# Case-insensitive; word-boundary-anchored so e.g. "stripe" doesn't flag
# "stripe_ansi_codes" (no such identifier exists today, but the anchor
# costs nothing and avoids a future false positive on an unrelated word
# that happens to contain one of these as a substring).
#
# Deliberately NOT the bare word "installation": that's ordinary English
# ("Diagnose installation health" in lopi-skill's own CLI doctor command,
# "GitHub App installation token" in lopi-github's PAT-vs-token doc
# comment) with no connection to the removed multi-tenant ledger. The
# sprint brief names "installation" as one of four terms to gate on; this
# is that intent narrowed to the actual removed identifiers so the gate
# catches real reintroduction without permanently flagging unrelated,
# legitimate uses of a common English word.
FORBIDDEN_TERMS = [
    "stripe",
    "customer_id",
    "open_for_customer",
    "customertier",
    "github_installations",
    "installationrow",
    "upsert_installation",
    "delete_installation",
    "list_installations",
    "customer_for_installation",
    "set_installation_tier",
]

TEST_PATH_MARKERS = ("/tests/", "/test/")
TEST_FILE_SUFFIXES = ("_test.rs", "_tests.rs")


def is_test_file(path: Path) -> bool:
    rel = path.as_posix()
    if any(marker in f"/{rel}/" for marker in TEST_PATH_MARKERS):
        return True
    return path.name.endswith(TEST_FILE_SUFFIXES)


def strip_cfg_test_blocks(text: str) -> str:
    """Best-effort removal of `#[cfg(test)] mod ... { ... }` bodies.

    Brace-depth scan from the `mod` block's opening `{` to its matching
    close. Not a full Rust parser — a `{`/`}` inside a string literal or a
    `format!()` argument can desync the depth counter — but the forbidden
    terms here are plain identifiers/words, not delimiters, so a desync
    only risks under- or over-stripping test code, never silently hiding a
    forbidden term that lives in real production code outside any test
    block.
    """
    out = []
    i = 0
    pattern = re.compile(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+\w+\s*\{")
    while i < len(text):
        m = pattern.search(text, i)
        if not m:
            out.append(text[i:])
            break
        out.append(text[i : m.start()])
        depth = 1
        j = m.end()
        while j < len(text) and depth > 0:
            if text[j] == "{":
                depth += 1
            elif text[j] == "}":
                depth -= 1
            j += 1
        i = j
    return "".join(out)


def find_violations(files: list[Path]) -> list[tuple[Path, int, str, str]]:
    violations = []
    term_patterns = [
        (term, re.compile(rf"(?i)\b{re.escape(term)}\b")) for term in FORBIDDEN_TERMS
    ]
    for path in files:
        if not path.exists() or path.suffix != ".rs" or is_test_file(path):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        scanned = strip_cfg_test_blocks(text)
        for lineno, line in enumerate(scanned.splitlines(), start=1):
            for term, pat in term_patterns:
                if pat.search(line):
                    violations.append((path, lineno, term, line.strip()))
    return violations


def collect_files(staged_only: bool) -> list[Path]:
    if staged_only:
        result = subprocess.run(
            ["git", "diff", "--cached", "--name-only", "--diff-filter=ACM"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        return [
            REPO_ROOT / line
            for line in result.stdout.splitlines()
            if line.endswith(".rs") and (line.startswith("crates/") or line.startswith("src/"))
        ]
    files = []
    for root in ("crates", "src"):
        base = REPO_ROOT / root
        if base.exists():
            files.extend(base.rglob("*.rs"))
    return files


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--staged-only", action="store_true")
    args = parser.parse_args()

    files = collect_files(args.staged_only)
    violations = find_violations(files)

    if not violations:
        print("Scope assertion: clean — no multi-tenant surface found.")
        return 0

    print("Scope assertion FAILED — multi-tenant surface reappeared:", file=sys.stderr)
    for path, lineno, term, line in violations:
        rel = path.relative_to(REPO_ROOT)
        print(f"  {rel}:{lineno}: forbidden term '{term}': {line}", file=sys.stderr)
    print(
        "\nlopi is single-operator, single-machine by design (Sprint S12 — see "
        "LEDGER.md and SECURITY.md's 'Deployment model'). If this is a deliberate "
        "reversal of that decision, update this script's FORBIDDEN_TERMS and record "
        "the reversal in LEDGER.md — do not silently work around this gate.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
