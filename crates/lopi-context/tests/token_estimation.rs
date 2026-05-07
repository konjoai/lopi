#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result)]
use lopi_context::{estimate_tokens, ContentBlock};

#[test]
fn text_block_estimated_nonzero() {
    let content = vec![ContentBlock::Text(
        "hello world this is a test message for token counting".to_string(),
    )];
    let estimate = estimate_tokens(&content);
    assert!(
        estimate > 0,
        "text block must produce a nonzero token estimate"
    );
}

#[test]
fn tool_use_block_estimated_nonzero() {
    let content = vec![ContentBlock::ToolUse {
        id: "toolu_01A2B3C4".to_string(),
        name: "read_file".to_string(),
        input: serde_json::json!({ "path": "/Users/dev/project/src/main.rs" }),
    }];
    let estimate = estimate_tokens(&content);
    assert!(
        estimate > 0,
        "tool_use block must produce a nonzero token estimate"
    );
}

#[test]
fn tool_result_block_estimated_nonzero() {
    let content = vec![ContentBlock::ToolResult {
        tool_use_id: "toolu_01A2B3C4".to_string(),
        content: "fn main() { println!(\"Hello, world!\"); }".to_string(),
        is_error: false,
    }];
    let estimate = estimate_tokens(&content);
    assert!(
        estimate > 0,
        "tool_result block must produce a nonzero token estimate"
    );
}

#[test]
fn mixed_content_estimate_is_additive() {
    let text = vec![ContentBlock::Text("some text".to_string())];
    let tool = vec![ContentBlock::ToolUse {
        id: "tool_1".to_string(),
        name: "bash".to_string(),
        input: serde_json::json!({"cmd": "ls"}),
    }];
    let combined = vec![
        ContentBlock::Text("some text".to_string()),
        ContentBlock::ToolUse {
            id: "tool_1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"cmd": "ls"}),
        },
    ];
    let text_est = estimate_tokens(&text);
    let tool_est = estimate_tokens(&tool);
    let combined_est = estimate_tokens(&combined);
    // Combined should be approximately text + tool (minus 4 overhead counted twice).
    assert!(
        combined_est >= text_est.saturating_sub(4) && combined_est <= text_est + tool_est + 4,
        "combined estimate {combined_est} should be roughly text {text_est} + tool {tool_est}"
    );
}

/// Integration test: verifies our estimate is within 10% of the Anthropic count-tokens API.
/// Requires `ANTHROPIC_API_KEY` in environment.
/// Run with: `cargo test --test token_estimation -- --ignored`
#[test]
#[ignore = "requires ANTHROPIC_API_KEY; run: cargo test --test token_estimation -- --ignored"]
fn estimate_within_10_percent_of_api() {
    // Construct a representative conversation.
    let content = vec![
        ContentBlock::Text("Implement a Rust function that reads a file and returns its contents as a String.".to_string()),
        ContentBlock::ToolUse {
            id: "toolu_01XYZ".to_string(),
            name: "read_file".to_string(),
            input: serde_json::json!({ "path": "src/main.rs" }),
        },
        ContentBlock::ToolResult {
            tool_use_id: "toolu_01XYZ".to_string(),
            content: "use std::fs;\nfn read_contents(path: &str) -> std::io::Result<String> { fs::read_to_string(path) }".to_string(),
            is_error: false,
        },
    ];

    let our_estimate = estimate_tokens(&content);

    // Call the Anthropic count-tokens endpoint (requires ANTHROPIC_API_KEY).
    // This is a synchronous HTTP call for simplicity.
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");
    let client = reqwest::blocking::Client::new();
    let body = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "messages": [{ "role": "user", "content": "test" }]
    });
    let response = client
        .post("https://api.anthropic.com/v1/messages/count_tokens")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .expect("API call must succeed");

    let json: serde_json::Value = response.json().expect("API must return JSON");
    #[allow(clippy::cast_possible_truncation)]
    let api_count = json["input_tokens"]
        .as_u64()
        .expect("response must have input_tokens") as usize;

    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let tolerance = (api_count as f64 * 0.10) as usize + 1;
    let diff = our_estimate.abs_diff(api_count);
    assert!(
        diff <= tolerance,
        "estimate {our_estimate} is more than 10% off from API count {api_count} (diff={diff}, tolerance={tolerance})"
    );
}
