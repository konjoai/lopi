#!/usr/bin/env python3
"""indexing_floor_check: ratchet on raw `[0]`/`[1]` indexing sites in production Rust.

Sprint S13R, Phase D. A raw index (`v[0]`, `xs[1]`) panics on an out-of-bounds slice
instead of returning `Option`/`Result` the way `.first()`/`.get(1)` would — this is a
standing panic-surface count, not a claim that every site is wrong (some are genuinely
bounds-checked upstream), ratcheted the same way the coverage floor is: lock the current
count, never regress above it, ratchet down as sites get converted to fallible accessors.

Method (stated precisely because a prior informal count of "202" for this same class of
site did not reproduce under a looser filter — see LEDGER.md's `Indexing-Floor-Seed-1`):
count *occurrences* (not lines) of the literal substrings `[0]` or `[1]` in every `.rs`
file under `crates/` and `src/`, excluding:
  - any path containing a `/tests/` or `/benches/` directory segment
  - any file named `tests.rs`, or ending in `_tests.rs`, `_test.rs`, or `_bench.rs`
  - lines whose first non-whitespace characters are `//` (a comment-only line)
This is a lint-style grep, not a parser — it does not know `[0]` inside a string literal
or a doc-link footnote from a real slice index. Good enough for a ratchet floor; not a
claim of byte-exact precision.

Usage: python3 .konjo/scripts/indexing_floor_check.py [--ceiling-file PATH]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SCAN_DIRS = ("crates", "src")
_INDEX_RE = re.compile(r"\[0\]|\[1\]")
_TEST_DIR_MARKERS = ("/tests/", "/benches/")
_TEST_FILE_NAMES = {"tests.rs"}
_TEST_FILE_SUFFIXES = ("_tests.rs", "_test.rs", "_bench.rs")


def _is_test_path(rel: str) -> bool:
    if any(marker in f"/{rel}" for marker in _TEST_DIR_MARKERS):
        return True
    name = Path(rel).name
    return name in _TEST_FILE_NAMES or name.endswith(_TEST_FILE_SUFFIXES)


def count_indexing_sites() -> tuple[int, list[str]]:
    total = 0
    hits: list[str] = []
    for scan_dir in _SCAN_DIRS:
        base = REPO_ROOT / scan_dir
        if not base.exists():
            continue
        for path in sorted(base.rglob("*.rs")):
            rel = str(path.relative_to(REPO_ROOT))
            if "target" in Path(rel).parts or _is_test_path(rel):
                continue
            try:
                text = path.read_text(encoding="utf-8")
            except (OSError, UnicodeDecodeError):
                continue
            for lineno, line in enumerate(text.splitlines(), start=1):
                if line.strip().startswith("//"):
                    continue
                n = len(_INDEX_RE.findall(line))
                if n:
                    total += n
                    hits.append(f"{rel}:{lineno} (+{n})")
    return total, hits


def _read_ceiling(ceiling_path: Path) -> int:
    for raw_line in ceiling_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        return int(line)
    raise ValueError(f"{ceiling_path} has no ceiling value (only comments/blank lines)")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ceiling-file", type=Path, default=None)
    args = parser.parse_args(argv)

    total, hits = count_indexing_sites()
    print(f"indexing_floor: {total} raw `[0]`/`[1]` site(s) in production code")

    if args.ceiling_file is None:
        return 0

    try:
        ceiling = _read_ceiling(args.ceiling_file)
    except (OSError, ValueError) as exc:
        print(f"::error::Cannot read indexing ceiling from {args.ceiling_file}: {exc}")
        return 2

    print(f"Locked ceiling: {ceiling}")
    if total > ceiling:
        print(
            f"::error::{total} indexing site(s), up from the locked ceiling of {ceiling} "
            f"({args.ceiling_file}). Convert the new/grown site(s) to `.first()`/`.get(n)`, "
            "or if this is a genuine ratchet-down PR, say why in the commit message and "
            "lower the ceiling -- never raise it silently."
        )
        return 1
    if total < ceiling:
        print(
            f"Count dropped {ceiling - total} below the ceiling. Consider ratcheting "
            f"{args.ceiling_file} down to {total} in this PR."
        )
    print("indexing floor gate: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
