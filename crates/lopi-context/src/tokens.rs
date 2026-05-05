use crate::types::ContentBlock;
use std::sync::OnceLock;
use tiktoken_rs::CoreBPE;

static BPE: OnceLock<Option<CoreBPE>> = OnceLock::new();

fn get_bpe() -> Option<&'static CoreBPE> {
    BPE.get_or_init(|| tiktoken_rs::cl100k_base().ok()).as_ref()
}

/// Estimate token count for a message's content blocks.
///
/// Text blocks: cl100k_base BPE (falls back to byte/4 if BPE unavailable).
/// ToolUse blocks: BPE(name) + BPE(id) + json_bytes/4.
/// ToolResult blocks: BPE(tool_use_id) + content_bytes/4.
/// Adds 4 tokens of role/structure overhead per message.
pub fn estimate_tokens(content: &[ContentBlock]) -> usize {
    let mut total = 4usize; // role + structure overhead
    for block in content {
        total += match block {
            ContentBlock::Text(s) => get_bpe()
                .map(|bpe| bpe.encode_with_special_tokens(s).len())
                .unwrap_or_else(|| s.len() / 4),

            ContentBlock::ToolUse { id, name, input } => {
                let json = serde_json::to_string(input).unwrap_or_default();
                let name_tokens = get_bpe()
                    .map(|bpe| bpe.encode_with_special_tokens(name).len())
                    .unwrap_or_else(|| name.len() / 4);
                let id_tokens = get_bpe()
                    .map(|bpe| bpe.encode_with_special_tokens(id).len())
                    .unwrap_or_else(|| id.len() / 4);
                name_tokens + id_tokens + json.len() / 4
            }

            ContentBlock::ToolResult { tool_use_id, content, .. } => {
                let id_tokens = get_bpe()
                    .map(|bpe| bpe.encode_with_special_tokens(tool_use_id).len())
                    .unwrap_or_else(|| tool_use_id.len() / 4);
                id_tokens + content.len() / 4
            }
        };
    }
    total
}
