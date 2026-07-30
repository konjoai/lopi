#!/usr/bin/env python3
"""function_length_check: mechanize CLAUDE.md's "function body <= 50 lines" claim.

Sprint S13R, Phase B, decision item 3: the original claim ("Function body <= 50 lines
(30 target)") had zero mechanical enforcement anywhere -- only a WARNING-tier question
in the Wall-3 LLM review that cannot block a merge. Per that phase's own choice ("write
the gate, or drop the claim"), this is the gate.

A lint-style scan, not a real parser: naive brace counting over `.rs` source text, the
same simplification this repo's other `.konjo/scripts/*.py` checkers already make (see
`scope_assert.py`, `dry_check.py`). Good enough to catch a function that has clearly
grown past the limit; not a substitute for `rustc`/`syn` if a function's shape gets
adversarial (raw strings, byte literals containing brace characters).

Usage: python3 .konjo/scripts/function_length_check.py [--hard-limit 50] [--soft-target 30]
Exit 0 if every function's body is <= --hard-limit lines. Exit 1 otherwise, naming each
offender. Functions between --soft-target and --hard-limit print as a warning, not a
failure.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent

# Same test-file exclusion convention as the coverage/DRY/scope checkers: a test file's
# function-length is not a production-code-quality signal.
_TEST_PATH_MARKERS = ("/tests/", "/benches/")
_TEST_FILE_SUFFIXES = ("_tests.rs", "_test.rs", "_bench.rs", "tests.rs")

# `fn` declarations, optionally preceded by visibility/async/unsafe/const/extern
# modifiers. Deliberately does not try to match generics/where-clauses precisely --
# it only needs to find the line the signature starts on and then the opening brace.
_FN_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]*\"\s+)?"
    r"(?:const\s+)?fn\s+[A-Za-z_][A-Za-z0-9_]*"
)


def _is_test_path(path: Path) -> bool:
    rel = str(path.relative_to(REPO_ROOT)) if path.is_absolute() else str(path)
    if any(marker in f"/{rel}" for marker in _TEST_PATH_MARKERS):
        return True
    return rel.endswith(_TEST_FILE_SUFFIXES)


def _strip_line_comment(line: str) -> str:
    """Best-effort strip of a trailing `//` comment, ignoring `//` inside a string
    literal. Not string-aware beyond counting unescaped quotes -- good enough for the
    brace-counting use below, which only cares about `{`/`}` outside of comments."""
    in_string = False
    i = 0
    while i < len(line) - 1:
        ch = line[i]
        if ch == '"' and (i == 0 or line[i - 1] != "\\"):
            in_string = not in_string
        elif not in_string and line[i : i + 2] == "//":
            return line[:i]
        i += 1
    return line


def _find_body_start(lines: list[str], sig_start: int) -> int | None:
    """Index of the line carrying this function's opening `{`, or None for a
    signature-only declaration (a trait method with no body, ending in `;`)."""
    for i in range(sig_start, min(sig_start + 20, len(lines))):
        stripped = _strip_line_comment(lines[i])
        if ";" in stripped and "{" not in stripped:
            return None
        if "{" in stripped:
            return i
    return None


def _find_body_end(lines: list[str], start: int) -> int:
    line = _strip_line_comment(lines[start])
    open_pos = line.rfind("{")
    tail = line[open_pos + 1 :]
    depth = 1 + tail.count("{") - tail.count("}")
    if depth <= 0:
        return start
    for i in range(start + 1, len(lines)):
        clean = _strip_line_comment(lines[i])
        depth += clean.count("{") - clean.count("}")
        if depth <= 0:
            return i
    return len(lines) - 1


def scan_file(path: Path) -> list[tuple[str, int, int]]:
    """Return (function-signature, start line (1-indexed), body line count) for every
    function in `path` whose body exceeds nothing yet -- caller filters by threshold."""
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return []
    lines = text.splitlines()
    out: list[tuple[str, int, int]] = []
    i = 0
    while i < len(lines):
        if _FN_RE.match(lines[i]):
            body_start = _find_body_start(lines, i)
            if body_start is not None:
                body_end = _find_body_end(lines, body_start)
                body_len = body_end - body_start - 1  # lines strictly inside the braces
                sig = lines[i].strip()
                out.append((sig, i + 1, body_len))
                i = body_end
        i += 1
    return out


def _read_ceiling(ceiling_path: Path) -> int:
    for raw_line in ceiling_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        return int(line)
    raise ValueError(f"{ceiling_path} has no ceiling value (only comments/blank lines)")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--hard-limit", type=int, default=50)
    parser.add_argument("--soft-target", type=int, default=30)
    parser.add_argument(
        "--ceiling-file",
        type=Path,
        default=None,
        help="Konjo Forward Pillar 2 ratchet: fail only if the over-hard-limit count "
        "rises above the locked value in this file (never regress). Without this "
        "flag, any function over --hard-limit fails outright -- use --ceiling-file "
        "for an existing repo's pre-adoption baseline.",
    )
    args = parser.parse_args(argv)

    offenders: list[tuple[str, str, int, int]] = []
    warnings: list[tuple[str, str, int, int]] = []
    for path in sorted(REPO_ROOT.rglob("*.rs")):
        rel = path.relative_to(REPO_ROOT)
        if "target" in rel.parts:
            continue
        if _is_test_path(path):
            continue
        for sig, line, length in scan_file(path):
            if length > args.hard_limit:
                offenders.append((str(rel), sig, line, length))
            elif length > args.soft_target:
                warnings.append((str(rel), sig, line, length))

    if warnings:
        print(f"function_length: {len(warnings)} function(s) over the {args.soft_target}-line target (not blocking):")
        for rel, sig, line, length in warnings:
            print(f"  {rel}:{line}: {sig} ({length} lines)")

    print(f"function_length: {len(offenders)} function(s) over the {args.hard_limit}-line hard limit:")
    for rel, sig, line, length in offenders:
        print(f"  {rel}:{line}: {sig} ({length} lines)")

    if args.ceiling_file is None:
        return 1 if offenders else 0

    try:
        ceiling = _read_ceiling(args.ceiling_file)
    except (OSError, ValueError) as exc:
        print(f"::error::Cannot read function-length ceiling from {args.ceiling_file}: {exc}")
        return 2

    print(f"Locked ceiling: {ceiling}")
    if len(offenders) > ceiling:
        print(
            f"::error::{len(offenders)} function(s) over {args.hard_limit} lines, up from "
            f"the locked ceiling of {ceiling} ({args.ceiling_file}). Split the new/grown "
            "offender(s) into smaller functions, or if this is a genuine ratchet-down "
            "PR, say why in the commit message and lower the ceiling -- never raise it "
            "silently."
        )
        return 1
    if len(offenders) < ceiling:
        print(
            f"Count dropped {ceiling - len(offenders)} below the ceiling. Consider "
            f"ratcheting {args.ceiling_file} down to {len(offenders)} in this PR."
        )
    print("function_length ceiling gate: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
