//! Fuzz target for `lopi_agent::claude_events::parse_line` — the `claude`
//! CLI's `--output-format stream-json` line parser (Sprint S12, Phase 2; see
//! `crates/lopi-agent/src/claude_events.rs`, 483 LOC). Every line is agent
//! output ultimately derived from repository content the operator doesn't
//! fully control, so this is attacker-influenceable input.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(line) = std::str::from_utf8(data) else {
        return;
    };
    let _ = lopi_agent::claude_events::parse_line(line);
});
