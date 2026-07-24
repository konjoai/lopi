#!/usr/bin/env python3
"""Konjo soft-gate convention lint.

The Three-Wall framework allows a CI step to be `continue-on-error: true`
only when that softness is declared and either dated or permanent — never
silently. This script makes that convention (already followed by hand in
`konjo-gate.yml`) mechanical: it fails on any `continue-on-error: true` step
that lacks one of:

  - a `KNOWN DEBT, verified <date>` comment that also mentions a next step
    (the debt is real, dated, and has a stated path to closing it), or
  - an explicit `ADVISORY BY DESIGN` marker (the softness is permanent by
    design — e.g. an opinionated lint tier, or a defensive no-op whose
    failure cannot affect the actual verdict).

This is line/comment based, not a full YAML+GitHub-Actions-expression
parser: it trusts that a step's documenting comment is the contiguous block
of `#`-lines immediately above `continue-on-error:` (optionally continuing
onto a trailing same-line comment). That is the convention every existing
step in this file already follows; the point of this lint is to keep it
that way going forward, not to relax it.

Exit codes:
  0 — every continue-on-error: true step is declared
  1 — one or more undeclared soft gates found
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

CONTINUE_ON_ERROR_RE = re.compile(r"^\s*continue-on-error:\s*true\b")
KNOWN_DEBT_RE = re.compile(r"KNOWN DEBT,\s*verified\s+\d{4}(-\d{2}(-\d{2})?)?", re.IGNORECASE)
NEXT_STEP_RE = re.compile(r"next\s+step", re.IGNORECASE)
ADVISORY_RE = re.compile(r"ADVISORY BY DESIGN", re.IGNORECASE)
STEP_NAME_RE = re.compile(r"^\s*-\s*name:\s*(.+?)\s*$")
COMMENT_RE = re.compile(r"^\s*#(.*)$")


def _step_indent(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def _nearest_step_name(lines: list[str], idx: int) -> str:
    """Walk upward from idx to find the enclosing step's `- name:` line."""
    for i in range(idx, -1, -1):
        m = STEP_NAME_RE.match(lines[i])
        if m:
            return m.group(1)
    return "(unknown step)"


def _comment_block(lines: list[str], idx: int) -> str:
    """Text of the contiguous comment lines directly above `lines[idx]`,
    plus any trailing inline comment on `lines[idx]` itself."""
    parts: list[str] = []

    inline = lines[idx].split("continue-on-error:", 1)[-1]
    if "#" in inline:
        parts.append(inline.split("#", 1)[1])

    block: list[str] = []
    i = idx - 1
    while i >= 0:
        m = COMMENT_RE.match(lines[i])
        if not m:
            break
        block.append(m.group(1))
        i -= 1
    block.reverse()
    parts.extend(block)
    # Join with spaces, not newlines: prose in these comments wraps across
    # lines mid-phrase (e.g. "...right now. Next\n# step: migrate..."), and
    # a newline-joined block would silently miss "next step" split that way.
    return " ".join(parts)


def check_file(path: Path) -> list[tuple[int, str, str]]:
    """Return a list of (line_number, step_name, reason) violations."""
    lines = path.read_text(encoding="utf-8").splitlines()
    violations = []
    for idx, line in enumerate(lines):
        if not CONTINUE_ON_ERROR_RE.match(line):
            continue
        block = _comment_block(lines, idx)
        if ADVISORY_RE.search(block):
            continue
        if KNOWN_DEBT_RE.search(block) and NEXT_STEP_RE.search(block):
            continue
        step_name = _nearest_step_name(lines, idx)
        reason = (
            "no `ADVISORY BY DESIGN` marker and no `KNOWN DEBT, verified <date>` "
            "comment with a next step"
        )
        violations.append((idx + 1, step_name, reason))
    return violations


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "paths",
        nargs="*",
        default=["/dev/stdin"],
        help="Workflow YAML files to lint (default: all args required by caller)",
    )
    args = parser.parse_args(argv)

    total_violations = 0
    for raw_path in args.paths:
        path = Path(raw_path)
        if not path.is_file():
            print(f"::error::{path}: not a file")
            return 1
        violations = check_file(path)
        for line_no, step_name, reason in violations:
            print(f"::error::{path}:{line_no}: step {step_name!r} has undeclared "
                  f"continue-on-error: true — {reason}")
        total_violations += len(violations)

    if total_violations:
        print(f"\n{total_violations} undeclared soft gate(s) found.")
        return 1
    print("Soft-gate convention lint: OK — every continue-on-error: true step is declared.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
