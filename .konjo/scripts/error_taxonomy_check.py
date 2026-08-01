#!/usr/bin/env python3
"""error_taxonomy_check: per-crate ratchet on `anyhow::` usage in production Rust.

Sprint S13R migrated `lopi-core` and part of `lopi-git` off `anyhow::` onto
typed (`thiserror`) errors, per `.claude/rules/rust-conventions.md` ("Error
types: `thiserror` for library crates, `anyhow` for binary/glue code"), and
left `lopi-memory` untouched. Nothing held that migration in place -- a single
workspace-wide ratchet (the shape `indexing_floor_check.py` and
`function_length_check.py` both use) cannot do it either: a regression in an
already-migrated crate and zero progress in an untouched crate both just move
one total up or down, so the same total could hide a real regression behind
unrelated migration progress elsewhere. This ratchet is **per-crate** instead --
one floor per crate, compared independently, so a crate that already hit 0
can never silently regress even while other crates stay wherever they are.

Method (same filtering convention as `indexing_floor_check.py` and
`function_length_check.py`, so the three ratchets agree on what "non-test
Rust" means): for every crate directory directly under `crates/`, count the
*files* (not occurrences) under that crate's `src/` containing at least one
non-comment line with the literal substring `anyhow::`, excluding:
  - any path containing a `/tests/` or `/benches/` directory segment
  - any file named `tests.rs`, or ending in `_tests.rs`, `_test.rs`, or `_bench.rs`
  - lines whose first non-whitespace characters are `//` (a comment-only line)
A file counts once no matter how many `anyhow::` occurrences it has -- the
signal this ratchet cares about is "how many files still need migrating",
not "how many call sites". This does not look inside `#[cfg(test)] mod
tests { ... }` blocks embedded in an otherwise-production file, the same
simplification `indexing_floor_check.py`'s own docstring names -- a file
whose only match is inside such a block still counts. Good enough for a
ratchet floor; not a claim of byte-exact precision.

Floor file format (`.konjo/error-taxonomy.txt`): one `crate-name: N` row per
crate, comment lines starting with `#`, blank lines ignored.

Usage: python3 .konjo/scripts/error_taxonomy_check.py [--floor-file PATH]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_CRATES_DIR = "crates"
_ANYHOW_RE = re.compile(r"anyhow::")
_TEST_DIR_MARKERS = ("/tests/", "/benches/")
_TEST_FILE_NAMES = {"tests.rs"}
_TEST_FILE_SUFFIXES = ("_tests.rs", "_test.rs", "_bench.rs")
_ROW_RE = re.compile(r"^([A-Za-z0-9_-]+)\s*:\s*(\d+)\s*$")


def _is_test_path(rel: str) -> bool:
    if any(marker in f"/{rel}" for marker in _TEST_DIR_MARKERS):
        return True
    name = Path(rel).name
    return name in _TEST_FILE_NAMES or name.endswith(_TEST_FILE_SUFFIXES)


def _file_has_real_anyhow_usage(path: Path) -> bool:
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return False
    for line in text.splitlines():
        if line.strip().startswith("//"):
            continue
        if _ANYHOW_RE.search(line):
            return True
    return False


def count_anyhow_files_per_crate() -> dict[str, int]:
    """Map crate directory name -> count of non-test files using `anyhow::`.

    Every crate directory directly under `crates/` is included, even at 0,
    so the floor file has somewhere to record "still clean" as well as
    "still migrating" -- a crate absent from this dict entirely means
    `crates/<name>` doesn't exist (or has no `.rs` files at all).
    """
    counts: dict[str, int] = {}
    base = REPO_ROOT / _CRATES_DIR
    if not base.exists():
        return counts
    for crate_dir in sorted(p for p in base.iterdir() if p.is_dir()):
        crate = crate_dir.name
        total = 0
        for path in sorted(crate_dir.rglob("*.rs")):
            rel = str(path.relative_to(REPO_ROOT))
            if "target" in Path(rel).parts or _is_test_path(rel):
                continue
            if _file_has_real_anyhow_usage(path):
                total += 1
        counts[crate] = total
    return counts


def _read_floor(floor_path: Path) -> dict[str, int]:
    floor: dict[str, int] = {}
    for raw_line in floor_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        m = _ROW_RE.match(line)
        if not m:
            raise ValueError(f"malformed row (want `crate-name: N`): {raw_line!r}")
        floor[m.group(1)] = int(m.group(2))
    return floor


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--floor-file", type=Path, default=REPO_ROOT / ".konjo" / "error-taxonomy.txt"
    )
    args = parser.parse_args(argv)

    try:
        floor = _read_floor(args.floor_file)
    except (OSError, ValueError) as exc:
        print(f"::error::Cannot read error-taxonomy floor from {args.floor_file}: {exc}")
        return 2

    counts = count_anyhow_files_per_crate()

    print("error_taxonomy: non-test `anyhow::` files per crate")
    regressions: list[str] = []
    improved: list[str] = []
    unrecorded: list[str] = []
    for crate in sorted(counts):
        current = counts[crate]
        if crate not in floor:
            if current > 0:
                unrecorded.append(crate)
            print(f"  {crate}: {current} (no locked floor -- add a row to {args.floor_file.name})")
            continue
        locked = floor[crate]
        marker = ""
        if current > locked:
            marker = "  <-- REGRESSION"
            regressions.append(
                f"{crate}: {current} `anyhow::` file(s), up from the locked floor of {locked}"
            )
        elif current < locked:
            marker = "  (dropped below floor)"
            improved.append(f"{crate}: {locked} -> {current}")
        print(f"  {crate}: {current} (floor {locked}){marker}")

    if unrecorded:
        print(
            "::error::Crate(s) with `anyhow::` usage but no floor row: "
            + ", ".join(unrecorded)
            + f". Add each to {args.floor_file} before this can be ratcheted."
        )
        return 1

    if regressions:
        print("::error::error-taxonomy regression(s):")
        for msg in regressions:
            print(f"  {msg}")
        print(
            "Migrate the new/grown file(s) off `anyhow::` onto typed errors "
            "(see crates/lopi-core/src/config.rs or crates/lopi-git/src/diff.rs "
            "for the established `thiserror` pattern), or if this is a genuine "
            "ratchet-down PR, say why in the commit message and lower the floor "
            "-- never raise it silently."
        )
        return 1

    if improved:
        print("Count(s) dropped below the locked floor. Consider ratcheting down:")
        for msg in improved:
            print(f"  {msg}")

    print("error-taxonomy gate: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
