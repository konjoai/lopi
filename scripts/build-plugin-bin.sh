#!/usr/bin/env bash
# Build the `lopi` binary and drop it where `plugin/.mcp.json`'s
# `${CLAUDE_PLUGIN_ROOT}/bin/lopi` expects it.
#
# Track A (Claude Code plugin) ships the binary this way for now: a local
# release build, copied in. Prebuilt cross-platform binaries (so installing
# the plugin doesn't require a Rust toolchain) are Track B's job — see
# LOPI_DISTRIBUTION_PLAN.md — not this script.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo build --release
mkdir -p plugin/bin
cp target/release/lopi plugin/bin/lopi
chmod +x plugin/bin/lopi
echo "✓ plugin/bin/lopi built ($(plugin/bin/lopi --version))"
