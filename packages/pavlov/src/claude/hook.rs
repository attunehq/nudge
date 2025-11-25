//! Types and parsers for interacting with Claude Code hooks.

use std::path::PathBuf;

use bon::Builder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Hooks handled by Pavlov.
#[derive(Debug, Deserialize)]
#[serde(tag = "hook_event_name")]
pub enum Hook {
    /// Sent before a tool is used.
    PreToolUse(PreToolUsePayload),

    /// Sent after a tool is used.
    PostToolUse(PostToolUsePayload),

    /// Sent when the main agent finishes sending output.
    Stop(StopPayload),

    /// Sent when the user submits a prompt.
    UserPromptSubmit(UserPromptSubmitPayload),
}

/// Shared fields in all hook payloads.
#[derive(Debug, Deserialize)]
pub struct Context {
    pub session_id: String,
    pub transcript_path: PathBuf,
    pub permission_mode: String,
}

#[derive(Debug, Deserialize)]
pub struct PreToolUsePayload {
    #[serde(flatten)]
    pub context: Context,

    pub cwd: PathBuf,
    pub tool_name: String,
    pub tool_use_id: String,

    /// The input to the tool.
    ///
    /// This is a JSON object whose shape is determined by `tool_name`.
    pub tool_input: Value,
}

#[derive(Debug, Deserialize)]
pub struct PostToolUsePayload {
    #[serde(flatten)]
    pub context: Context,

    pub cwd: PathBuf,
    pub tool_name: String,
    pub tool_use_id: String,

    /// The input to the tool.
    ///
    /// This is a JSON object whose shape is determined by `tool_name`.
    pub tool_input: Value,

    /// The response from the tool.
    ///
    /// This is a JSON object whose shape is determined by `tool_name`.
    pub tool_response: Value,
}

#[derive(Debug, Deserialize)]
pub struct StopPayload {
    #[serde(flatten)]
    pub context: Context,

    pub stop_hook_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct UserPromptSubmitPayload {
    #[serde(flatten)]
    pub context: Context,

    pub cwd: PathBuf,
    pub prompt: String,
}

/// The response to a hook.
#[derive(Debug)]
#[non_exhaustive]
pub enum Response {
    /// Pass the operation through without modification.
    ///
    /// Returns exit code 0 and emits no data. Use when the hook has nothing
    /// to say about the operation.
    Passthrough,

    /// Block the operation and provide feedback.
    ///
    /// Returns exit code 2 and emits the response serialized as JSON. The tool
    /// use is **stopped**: Claude does not proceed with the operation.
    ///
    /// Use for hard rules where the code is clearly wrong and should be fixed
    /// before proceeding (e.g., imports inside function bodies, missing blank
    /// lines between struct fields).
    Interrupt(InterruptResponse),

    /// Allow the operation but inject guidance into the conversation.
    ///
    /// Returns exit code 0 and emits the response serialized as JSON. The tool
    /// use **proceeds**: Claude writes the file, but sees the guidance message.
    ///
    /// Use for soft suggestions where the code works but could be improved
    /// (e.g., stylistic preferences like turbofish vs LHS type annotations).
    Continue(ContinueResponse),
}

/// Hook-specific output for PreToolUse hooks.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PreToolUseOutput {
    /// Must be "PreToolUse" for Claude Code to accept the response.
    pub hook_event_name: PreToolUseEventName,
}

/// Marker type that serializes to "PreToolUse".
#[derive(Debug, Default)]
pub struct PreToolUseEventName;

impl Serialize for PreToolUseEventName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("PreToolUse")
    }
}

#[derive(Debug, Serialize, Builder)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ContinueResponse {
    /// Skipped so that the builder can't set it.
    ///
    /// Claude Code uses this field to determine whether to continue, but we use
    /// separate types to indicate this, so we hard code it into the type for
    /// easy serialization.
    #[builder(skip = true)]
    pub r#continue: bool,

    /// Whether to suppress the operation's output.
    #[builder(default = false)]
    pub suppress_output: bool,

    /// A system message to add to the transcript.
    #[builder(into)]
    pub system_message: String,

    /// A JSON object whose shape is determined by the hook.
    #[builder(into, default = Value::Object(Map::new()))]
    pub hook_specific_output: Value,
}

#[derive(Debug, Serialize, Builder)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct InterruptResponse {
    /// Skipped so that the builder can't set it.
    ///
    /// Claude Code uses this field to determine whether to continue, but we use
    /// separate types to indicate this, so we hard code it into the type for
    /// easy serialization.
    #[builder(skip = false)]
    pub r#continue: bool,

    /// The reason for interrupting the operation.
    #[builder(into)]
    pub stop_reason: String,

    /// Whether to suppress the operation's output.
    #[builder(default = false)]
    pub suppress_output: bool,

    /// A system message to add to the transcript.
    #[builder(into)]
    pub system_message: String,

    /// A JSON object whose shape is determined by the hook.
    #[builder(into, default = Value::Object(Map::new()))]
    pub hook_specific_output: Value,
}

/// Configures a hook in Claude Code's settings.json.
#[derive(Debug, Serialize, Clone, Builder)]
#[non_exhaustive]
pub struct Config {
    /// The type of hook to run. Valid options are `command` or `prompt`, but we
    /// always want `command`.
    #[builder(skip = String::from("command"))]
    pub r#type: String,

    /// The command to run.
    #[builder(into)]
    pub command: String,

    /// Terminate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

impl From<&Config> for Config {
    fn from(value: &Config) -> Self {
        value.clone()
    }
}

/// Configures hook matching strategy in Claude Code's settings.json.
#[derive(Debug, Serialize, Clone, Builder)]
#[non_exhaustive]
pub struct Matcher {
    /// The matcher to use for this hook.
    ///
    /// This is only used with with tool hooks: `PreToolUse`,
    /// `PermissionRequest`, and `PostToolUse`. For other hooks, it is
    /// ignored.
    #[builder(default = "", into)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub matcher: String,

    /// The hooks to run when the matcher matches.
    #[builder(with = |i: impl IntoIterator<Item = impl Into<Config>>| i.into_iter().map(Into::into).collect())]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Config>,
}
