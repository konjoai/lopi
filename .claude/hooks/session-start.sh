#!/bin/bash
# Sprint S13R, Phase F: enforcement from the first prompt. Prints the standing
# quality-gate floors so an agent starting a session sees the framework's real
# state immediately, not partway through, after already proposing work that
# would regress one of them. Report-only — never blocks session start, and
# never fails even if a floor file is momentarily missing (e.g. a fresh
# checkout mid-rebase).
ROOT="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$ROOT" || exit 0

echo "--- Konjo quality floors (ratchet: never regress above these) ---"
[ -f .konjo/coverage-floor.txt ] && echo "coverage floor:        $(grep -v '^#' .konjo/coverage-floor.txt | grep -v '^\s*$' | head -1)%"
[ -f .konjo/function-length-ceiling.txt ] && echo "function-length ceiling: $(grep -v '^#' .konjo/function-length-ceiling.txt | grep -v '^\s*$' | head -1) functions > 50 lines"
[ -f .konjo/indexing-floor.txt ] && echo "indexing floor:         $(grep -v '^#' .konjo/indexing-floor.txt | grep -v '^\s*$' | head -1) raw [0]/[1] sites"

PINNED=$(grep -v '^#' .konjo/kiban.ref 2>/dev/null | head -1)
CI_REF=$(grep -oE 'KIBAN_REF: "v[0-9.]+"' .github/workflows/konjo-gate.yml 2>/dev/null | head -1 | grep -oE 'v[0-9.]+')
if [ -n "$PINNED" ] && [ -n "$CI_REF" ] && [ "$PINNED" != "$CI_REF" ]; then
  echo "::warning:: .konjo/kiban.ref ($PINNED) and konjo-gate.yml's KIBAN_REF ($CI_REF) have drifted apart -- bump both together."
fi

exit 0
