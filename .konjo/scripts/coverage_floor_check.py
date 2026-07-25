#!/usr/bin/env python3
"""Konjo coverage floor gate — Konjo Forward Pillar 2 (a cleared bar never moves
backward).

Compares measured workspace line coverage against the floor recorded in
`.konjo/coverage-floor.txt` and fails if coverage regressed below it. The
80%/95% coverage gate in konjo-gate.yml stays soft (real coverage isn't there
yet); this floor is the number lopi has actually earned, and it is hard.

Coverage is read from `lcov.info` by summing `LF:`/`LH:` directly, the same
method the existing 80% gate uses — `cargo llvm-cov report --json` does not
support `--workspace` and silently under-scopes, so parsing the already
workspace-scoped lcov.info is the only correct source of truth here too.

Exit codes:
  0 — measured coverage >= floor
  1 — measured coverage < floor (regression) or lcov.info missing/unparseable
  2 — floor file missing or malformed
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def measure_lcov_coverage(lcov_path: Path) -> float:
    """Sum LF/LH across an lcov.info file and return percent coverage.

    Missing or empty input is treated as 0.0% — conservative, so a broken
    measurement fails the gate instead of silently passing it.
    """
    lf = lh = 0
    try:
        with lcov_path.open(encoding="utf-8") as f:
            for line in f:
                if line.startswith("LF:"):
                    lf += int(line.strip().split(":")[1])
                elif line.startswith("LH:"):
                    lh += int(line.strip().split(":")[1])
    except (OSError, ValueError):
        return 0.0
    return 100 * lh / lf if lf else 0.0


def read_floor(floor_path: Path) -> float:
    """Read the locked coverage floor: the first non-comment, non-blank line."""
    for raw_line in floor_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        return float(line)
    raise ValueError(f"{floor_path} has no floor value (only comments/blank lines)")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lcov", type=Path, default=Path("lcov.info"))
    parser.add_argument(
        "--floor-file", type=Path, default=Path(".konjo/coverage-floor.txt")
    )
    args = parser.parse_args(argv)

    try:
        floor = read_floor(args.floor_file)
    except (OSError, ValueError) as exc:
        print(f"::error::Cannot read coverage floor from {args.floor_file}: {exc}")
        return 2

    measured = measure_lcov_coverage(args.lcov)
    print(f"Measured line coverage: {measured:.2f}%")
    print(f"Locked floor:           {floor:.2f}%")

    # Round to 2dp before comparing — lcov's own precision — so a floor
    # ratcheted to the exact measured value doesn't fail on float noise.
    if round(measured, 2) < round(floor, 2):
        print(
            f"::error::Coverage {measured:.2f}% dropped below the locked floor "
            f"{floor:.2f}% ({args.floor_file}). Add tests to recover it, or if "
            "this is a genuine measurement fix, say why in the commit message "
            "and update the floor — never lower it silently."
        )
        return 1

    if measured > floor:
        print(
            f"Coverage rose {measured - floor:.2f}pp above the floor. Consider "
            f"ratcheting {args.floor_file} up to {measured:.2f}% in this PR."
        )
    print("Coverage floor gate: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
