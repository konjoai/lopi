use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a single conversation turn.
pub type TurnId = Uuid;
/// Shared identifier linking a `ToolUse` turn to its `ToolResult` partner.
pub type ToolPairId = Uuid;

/// Conversation participant — either the human user or the Claude assistant.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    /// Message originated from the human side.
    User,
    /// Message originated from the Claude assistant.
    Assistant,
}

/// Agent execution phase used to bucket turns for phase-transition eviction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Initial startup and configuration phase.
    Boot,
    /// Repository exploration and understanding phase.
    Discovery,
    /// Task planning and decomposition phase.
    Planning,
    /// Code writing and change application phase.
    Implementation,
    /// Test execution and validation phase.
    Testing,
    /// Result summarisation and PR preparation phase.
    Conclusion,
}

/// Controls when a turn may be evicted from the context window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PinPolicy {
    /// Never evict — kept for the entire agent run.
    Always,
    /// Keep until the window enters the given phase, then release.
    UntilPhase(Phase),
    /// Freely evictable by any policy.
    Never,
    /// Evictable only when the budget threshold is exceeded (LIFO order).
    BudgetEvictable,
}

/// A single content block within a message — text, tool invocation, or tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    /// Plain text content.
    Text(String),
    /// A tool invocation issued by the assistant.
    ToolUse {
        /// Opaque identifier linking this call to its result.
        id: String,
        /// Name of the tool being called.
        name: String,
        /// JSON-encoded input arguments.
        input: serde_json::Value,
    },
    /// The output returned by a tool call.
    ToolResult {
        /// The `id` from the matching `ToolUse` block.
        tool_use_id: String,
        /// Serialised tool output.
        content: String,
        /// True if the tool reported an error.
        is_error: bool,
    },
}

/// A conversation turn with metadata required for selective eviction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggedMessage {
    /// Unique turn identifier.
    pub id: TurnId,
    /// Who produced this message.
    pub role: Role,
    /// Message body — one or more content blocks.
    pub content: Vec<ContentBlock>,
    /// Token count — estimated on insert, never recomputed.
    pub tokens: usize,
    /// Eviction policy governing when this turn may be removed.
    pub pin: PinPolicy,
    /// Phase during which this turn was inserted.
    pub phase: Phase,
    /// Evict this turn after the turn with this ID is inserted.
    pub evict_after: Option<TurnId>,
    /// Links this turn to its `tool_use`/`tool_result` partner for atomic eviction.
    pub tool_pair_id: Option<ToolPairId>,
    /// Distilled phase summaries — never evicted by automatic policy.
    pub is_conclusion: bool,
}
