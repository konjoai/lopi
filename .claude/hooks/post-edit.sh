#!/bin/bash
# Runs language-appropriate lint/compile check after any source file edit.
# Claude sees the output and can fix errors immediately before continuing.
FILE=$(echo "$CLAUDE_TOOL_INPUT" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d.get("file_path",""))' 2>/dev/null)

if [[ "$FILE" == *.rs ]]; then
    echo "→ cargo check (triggered by edit to $FILE)"
    cd /Users/wesleyscholl/lopi && cargo check --quiet 2>&1 | head -30

elif [[ "$FILE" == *.py || "$FILE" == *.mojo ]]; then
    if command -v ruff &>/dev/null; then
        echo "→ ruff check (triggered by edit to $FILE)"
        ruff check --quiet "$FILE" 2>&1 | head -20
        ruff format --check --quiet "$FILE" 2>&1 | head -5
    fi
fi
