#!/bin/bash
# Runs language-appropriate lint/compile check after any source file edit.
# Claude sees the output and can fix errors immediately before continuing.
#
# Sprint S13R, Phase F: portable across machines/environments via
# $CLAUDE_PROJECT_DIR (falls back to resolving from this script's own path if
# unset, so it still works if a hook runner doesn't export it) -- no longer
# hardcoded to one contributor's home directory. Also extended to cover
# web/ (TypeScript/SvelteKit), the repo's second real language per CLAUDE.md's
# Stack line -- previously only .rs/.py/.mojo edits triggered a check.
ROOT="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"

FILE=$(echo "$CLAUDE_TOOL_INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("file_path",""))' 2>/dev/null)

if [[ "$FILE" == *.rs ]]; then
    echo "→ cargo check (triggered by edit to $FILE)"
    cd "$ROOT" && cargo check --quiet 2>&1 | head -30

elif [[ "$FILE" == *.py || "$FILE" == *.mojo ]]; then
    if command -v ruff &>/dev/null; then
        echo "→ ruff check (triggered by edit to $FILE)"
        ruff check --quiet "$FILE" 2>&1 | head -20
        ruff format --check --quiet "$FILE" 2>&1 | head -5
    fi

elif [[ "$FILE" == "$ROOT/web/"*.ts || "$FILE" == "$ROOT/web/"*.tsx || "$FILE" == "$ROOT/web/"*.svelte ]]; then
    if [ -d "$ROOT/web/node_modules" ]; then
        echo "→ svelte-check (triggered by edit to $FILE)"
        (cd "$ROOT/web" && npx --no-install svelte-check --tsconfig ./tsconfig.json --output human 2>&1 | tail -20)
    fi
fi
