//! NDJSON message types for Claude Code's stream-json format.

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// Output message from Claude Code (stdout).
///
/// Each line of stdout is a complete JSON object of one of these types.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum OutputMessage {
    /// System messages (init, etc.)
    #[serde(rename = "system")]
    System(SystemMessage),

    /// Assistant messages (text, tool use)
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),

    /// User messages (tool results)
    #[serde(rename = "user")]
    User(UserMessage),

    /// Result message (end of conversation turn)
    #[serde(rename = "result")]
    Result(ResultMessage),
}

/// System message, typically sent at conversation start.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SystemMessage {
    /// Subtype of the system message (e.g., "init")
    pub subtype: String,

    /// Session ID for this conversation
    pub session_id: Option<String>,

    /// Current working directory
    pub cwd: Option<String>,

    /// Model being used
    pub model: Option<String>,

    /// Available tools (we don't parse this in detail)
    #[serde(default)]
    pub tools: Vec<String>,
}

/// Assistant message containing response content.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct AssistantMessage {
    /// The message content (nested message object from Claude API)
    pub message: ApiMessage,

    /// Session ID
    pub session_id: Option<String>,
}

/// User message, typically containing tool results.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct UserMessage {
    /// The message content
    pub message: ApiMessage,

    /// Session ID
    pub session_id: Option<String>,
}

/// A message from the Claude API (nested inside assistant/user messages).
#[derive(Debug, Deserialize, Serialize)]
pub struct ApiMessage {
    /// Role of the message sender
    #[serde(default)]
    pub role: Option<String>,

    /// Content blocks
    pub content: MessageContent,

    /// Stop reason (if finished)
    pub stop_reason: Option<String>,
}

/// Message content can be a string or array of content blocks.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple string content
    Text(String),

    /// Array of content blocks
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    /// Iterate over content blocks, treating string content as a single text
    /// block.
    pub fn blocks(&self) -> impl Iterator<Item = ContentBlock> + '_ {
        match self {
            MessageContent::Text(s) => vec![ContentBlock::Text { text: s.clone() }].into_iter(),
            MessageContent::Blocks(blocks) => blocks.clone().into_iter(),
        }
    }
}

/// A content block within a message.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },

    /// Tool use request
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Tool result
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: ToolResultContent,
    },
}

/// Tool result content can be a string or structured.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// Simple string result
    Text(String),

    /// Structured result (we just preserve as JSON)
    Structured(serde_json::Value),
}

impl Display for ToolResultContent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolResultContent::Text(s) => write!(f, "{}", s),
            ToolResultContent::Structured(v) => {
                write!(f, "{}", serde_json::to_string_pretty(v).unwrap_or_default())
            }
        }
    }
}

/// Result message indicating end of a conversation turn.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ResultMessage {
    /// Result subtype (e.g., "success", "error")
    pub subtype: String,

    /// Session ID
    pub session_id: Option<String>,

    /// Whether this is an error
    #[serde(default)]
    pub is_error: bool,

    /// Total duration in milliseconds
    pub duration_ms: Option<u64>,

    /// API duration in milliseconds
    pub duration_api_ms: Option<u64>,

    /// Number of conversation turns
    pub num_turns: Option<u32>,

    /// Total cost in USD
    pub total_cost_usd: Option<f64>,

    /// Result text (for success)
    pub result: Option<String>,
}

/// Input message to send to Claude Code (stdin).
#[derive(Debug, Serialize)]
pub struct InputMessage {
    /// Message type (always "user" for input)
    pub r#type: String,

    /// The message content
    pub message: InputMessageContent,
}

/// Content of an input message.
#[derive(Debug, Serialize)]
pub struct InputMessageContent {
    /// Role (always "user")
    pub role: String,

    /// Content blocks
    pub content: Vec<InputContentBlock>,
}

/// A content block for input messages.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum InputContentBlock {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
}

impl InputMessage {
    /// Create a new user text message.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            r#type: "user".to_string(),
            message: InputMessageContent {
                role: "user".to_string(),
                content: vec![InputContentBlock::Text { text: text.into() }],
            },
        }
    }
}
