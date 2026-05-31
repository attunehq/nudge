//! Normalized hook event model and evaluation.

use std::path::PathBuf;

use serde_json::Value;

use crate::agent::AgentKind;

pub mod apply_patch;
pub mod evaluate;
pub mod response;
pub mod state;

/// A Nudge hook event after provider-specific payloads have been normalized.
#[derive(Debug, Clone, PartialEq)]
pub enum NudgeHook {
    /// A tool is about to be used.
    PreToolUse(PreToolUse),

    /// The agent is requesting permission for a tool.
    PermissionRequest(PermissionRequest),

    /// The user submitted a prompt.
    UserPromptSubmit(UserPromptSubmit),

    /// The agent is trying to stop.
    Stop(Stop),

    /// A hook event Nudge does not currently handle.
    Other,
}

/// Shared context that is useful across agent hook providers.
#[derive(Debug, Clone, PartialEq)]
pub struct HookContext {
    /// The provider that emitted the hook.
    pub agent: AgentKind,

    /// Provider session id, when available.
    pub session_id: Option<String>,

    /// Provider turn id, when available.
    pub turn_id: Option<String>,

    /// Transcript path, when available.
    pub transcript_path: Option<PathBuf>,

    /// Current working directory for the hook invocation.
    pub cwd: PathBuf,

    /// Provider permission mode, when available.
    pub permission_mode: Option<String>,

    /// Model name, when available.
    pub model: Option<String>,
}

/// Normalized `PreToolUse` event.
#[derive(Debug, Clone, PartialEq)]
pub struct PreToolUse {
    /// Shared hook context.
    pub context: HookContext,

    /// Original provider tool input.
    ///
    /// Nudge evaluates normalized tool shapes, but provider rewrite responses
    /// must return the complete tool input object with only the intended fields
    /// changed.
    pub tool_input: Value,

    /// Normalized tool input.
    pub tool: ToolUse,
}

/// Normalized parsed-but-unmatchable permission request.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRequest {
    /// Shared hook context.
    pub context: HookContext,

    /// Normalized requested tool input.
    pub tool: ToolUse,
}

/// Normalized `UserPromptSubmit` event.
#[derive(Debug, Clone, PartialEq)]
pub struct UserPromptSubmit {
    /// Shared hook context.
    pub context: HookContext,

    /// User prompt text.
    pub prompt: String,
}

/// Normalized `Stop` event.
#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    /// Shared hook context.
    pub context: HookContext,

    /// Whether this stop hook is already a continuation attempt.
    pub stop_hook_active: bool,

    /// Latest assistant message text, when the provider supplies it.
    pub last_assistant_message: Option<String>,
}

/// Normalized tool use.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolUse {
    /// Write file content.
    Write(WriteInput),

    /// Edit file content.
    Edit(EditInput),

    /// Delete a file.
    Delete(DeleteInput),

    /// Fetch a URL.
    WebFetch(WebFetchInput),

    /// Run a shell command.
    Bash(BashInput),

    /// Any unsupported tool.
    Other {
        /// Provider tool name.
        tool_name: String,

        /// Raw provider input.
        input: Value,
    },
}

/// Normalized write input.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteInput {
    /// Path being written.
    pub file_path: PathBuf,

    /// Full content being written.
    pub content: String,
}

/// Normalized edit input.
#[derive(Debug, Clone, PartialEq)]
pub struct EditInput {
    /// Path being edited.
    pub file_path: PathBuf,

    /// Replaced content or matched pre-patch content.
    pub old_string: String,

    /// Full post-edit content.
    pub new_string: String,
}

/// Normalized delete input.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteInput {
    /// Path being deleted.
    pub file_path: PathBuf,
}

/// Normalized web fetch input.
#[derive(Debug, Clone, PartialEq)]
pub struct WebFetchInput {
    /// URL being fetched.
    pub url: String,

    /// Provider prompt, when present.
    pub prompt: Option<String>,
}

/// Normalized bash input.
#[derive(Debug, Clone, PartialEq)]
pub struct BashInput {
    /// Command being executed.
    pub command: String,

    /// Human-readable description, when present.
    pub description: Option<String>,
}
