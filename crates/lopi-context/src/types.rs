use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type TurnId = Uuid;
pub type ToolPairId = Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Phase {
    Boot,
    Discovery,
    Planning,
    Implementation,
    Testing,
    Conclusion,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PinPolicy {
    Always,
    UntilPhase(Phase),
    Never,
    BudgetEvictable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggedMessage {
    pub id: TurnId,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    /// Token count — estimated on insert, never recomputed.
    pub tokens: usize,
    pub pin: PinPolicy,
    pub phase: Phase,
    /// Evict this turn after the turn with this ID is inserted.
    pub evict_after: Option<TurnId>,
    /// Links this turn to its tool_use/tool_result partner for atomic eviction.
    pub tool_pair_id: Option<ToolPairId>,
    /// Distilled phase summaries — never evicted by automatic policy.
    pub is_conclusion: bool,
}
