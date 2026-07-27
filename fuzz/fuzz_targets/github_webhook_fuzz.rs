//! Fuzz target for the GitHub webhook body parsing/field-extraction path
//! (Sprint S12, Phase 2; see `crates/lopi-webhook/src/github.rs`, 266 LOC).
//! Runs pre-HMAC-verification, for parse purposes only — this exercises
//! `fuzz_parse_and_extract`, a thin wrapper mirroring `handle`/
//! `dispatch_event`'s own field-access chains without the async `TaskQueue`
//! side effect (see that function's doc comment in `github.rs`).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    lopi_webhook::github::fuzz_parse_and_extract(data, "issues");
    lopi_webhook::github::fuzz_parse_and_extract(data, "pull_request_review");
    lopi_webhook::github::fuzz_parse_and_extract(data, "check_run");
});
