//! Fuzz target for `lopi_mcp::jsonrpc::Response` — the MCP wire type
//! deserialized from an unvetted MCP server's stdio replies (Sprint S12,
//! Phase 2; see `crates/lopi-mcp/src/jsonrpc.rs`, 233 LOC).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(resp) = serde_json::from_str::<lopi_mcp::jsonrpc::Response>(s) {
        // Exercise the one piece of real logic on the parsed value, same as
        // every production caller does with a `Response`.
        let _ = resp.into_result();
    }
});
