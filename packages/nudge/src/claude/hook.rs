//! Types and parsers for interacting with Claude Code hooks.

use std::path::PathBuf;

use bon::Builder;
use color_eyre::eyre::Result;
use derive_more::{AsRef, Display};
use serde::{Deserialize, Serialize, Serializer};

use crate::{
    rules::{
        ContentMatcher, PreToolUseEditMatcher, PreToolUseWriteMatcher, UserPromptSubmitMatcher,
    },
    snippet::{Source, Span},
};

/// Claude Code hooks handled by Nudge.
#[derive(Debug, Deserialize)]
#[serde(tag = "hook_event_name")]
pub enum Hook {
    /// Sent before a tool is used.
    PreToolUse(PreToolUsePayload),

    /// Sent when the user submits a prompt.
    UserPromptSubmit(UserPromptSubmitPayload),
}

impl Hook {
    /// The source snippet evaluated by the hook.
    pub fn source(&self) -> Source {
        match self {
            Hook::PreToolUse(payload) => match payload {
                PreToolUsePayload::Write(payload) => Source::from(&payload.tool_input.content),
                PreToolUsePayload::Edit(payload) => Source::from(&payload.tool_input.new_string),
            },
            Hook::UserPromptSubmit(payload) => Source::from(&payload.prompt),
        }
    }
}

/// Shared fields in all Claude Code hook payloads.
#[derive(Debug, Deserialize)]
pub struct Context {
    /// The session ID.
    pub session_id: String,

    /// The path to the chat transcript.
    pub transcript_path: PathBuf,

    /// The permission mode for the chat.
    pub permission_mode: String,

    /// The current working directory.
    pub cwd: PathBuf,
}

/// Payload for the `PreToolUse` hook.
#[derive(Debug, Deserialize)]
#[serde(tag = "tool_name")]
pub enum PreToolUsePayload {
    /// The Write tool.
    Write(PreToolUseWritePayload),

    /// The Edit tool.
    Edit(PreToolUseEditPayload),
}

/// Payload for the `Edit` tool.
#[derive(Debug, Deserialize)]
pub struct PreToolUseEditPayload {
    /// The context of the hook.
    #[serde(flatten)]
    pub context: Context,

    /// The ID of the tool use.
    pub tool_use_id: String,

    /// The input to the tool.
    pub tool_input: PreToolUseEditInput,
}

impl PreToolUseEditPayload {
    /// Evaluate the payload against the given rule.
    ///
    /// Returns the spans of all matches if the rule matched the payload.
    pub fn evaluate(&self, matcher: &PreToolUseEditMatcher) -> Vec<Span> {
        if matcher.file.is_match_path(&self.tool_input.file_path) {
            evaluate_all_matched(&self.tool_input.new_string, &matcher.new_content)
        } else {
            Vec::new()
        }
    }
}

/// Input for the `Edit` tool.
#[derive(Debug, Deserialize)]
pub struct PreToolUseEditInput {
    /// The path to the file to edit.
    pub file_path: PathBuf,

    /// The old content to replace.
    pub old_string: String,

    /// The new content to write.
    pub new_string: String,
}

/// Payload for the `Write` tool.
#[derive(Debug, Deserialize)]
pub struct PreToolUseWritePayload {
    /// The context of the hook.
    #[serde(flatten)]
    pub context: Context,

    /// The ID of the tool use.
    pub tool_use_id: String,

    /// The input to the tool.
    pub tool_input: PreToolUseWriteInput,
}

impl PreToolUseWritePayload {
    /// Evaluate the payload against the given rule.
    ///
    /// Returns the spans of all matches if the rule matched the payload.
    pub fn evaluate(&self, matcher: &PreToolUseWriteMatcher) -> Vec<Span> {
        if matcher.file.is_match_path(&self.tool_input.file_path) {
            evaluate_all_matched(&self.tool_input.content, &matcher.content)
        } else {
            Vec::new()
        }
    }
}

/// Input for the `Write` tool.
#[derive(Debug, Deserialize)]
pub struct PreToolUseWriteInput {
    /// The path to the file to write to.
    pub file_path: PathBuf,

    /// The content to write to the file.
    pub content: String,
}

/// Payload for the `UserPromptSubmit` hook.
#[derive(Debug, Deserialize)]
pub struct UserPromptSubmitPayload {
    /// The context of the hook.
    #[serde(flatten)]
    pub context: Context,

    /// The user's prompt.
    pub prompt: String,
}

impl UserPromptSubmitPayload {
    /// Evaluate the payload against the given rule.
    ///
    /// Returns the spans of all matches if the rule matched the payload.
    pub fn evaluate(&self, matcher: &UserPromptSubmitMatcher) -> Vec<Span> {
        evaluate_all_matched(&self.prompt, &matcher.prompt)
    }
}

/// The response to a `PreToolUse` hook.
#[derive(Debug)]
pub enum PreToolUseResponse {
    /// Pass the operation through without modification.
    Passthrough,

    /// Block the operation and provide feedback to Claude.
    Interrupt(PreToolUseInterruptResponse),
}

/// Interrupt a `PreToolUse` hook and provides feedback to Claude Code.
#[derive(Debug, Clone, Builder)]
pub struct PreToolUseInterruptResponse {
    /// The reason for the decision being made, displayed to Claude Code.
    #[builder(into)]
    model_feedback: String,

    /// The message to display to the user when this response is emitted.
    #[builder(into, default = "Nudge blocked operation due to rule violation")]
    user_message: String,
}

/// Claude Code expects a two-level structured response, but that's annoying to
/// work with for our use case, so we just translate it inside this `Serialize`
/// implementation.
impl Serialize for PreToolUseInterruptResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let output = PreToolUseHookSpecificOutput::builder()
            .permission_decision_reason(&self.model_feedback)
            .build();
        let envelope = HookResponseEnvelope::builder()
            .system_message(&self.user_message)
            .hook_specific_output(output)
            .build();
        envelope.serialize(serializer)
    }
}

/// The response to a `UserPromptSubmit` hook.
#[derive(Debug, Clone, AsRef, Display)]
pub struct UserPromptSubmitResponse(String);

impl<S: Into<String>> From<S> for UserPromptSubmitResponse {
    fn from(value: S) -> Self {
        UserPromptSubmitResponse(value.into())
    }
}

/// The top-level structure of a Claude Code hook response.
#[derive(Debug, Serialize, Clone, Builder)]
#[serde(rename_all = "camelCase")]
struct HookResponseEnvelope<T> {
    /// Whether Claude Code should continue after hook execution.
    ///
    /// This should nearly always be `true` so that Claude Code can respond to
    /// the hook event- for example, don't use this to reject a `PreToolUse`
    /// hook unless you want Claude Code to immediately abort the operation and
    /// do nothing else until the user prompts again.
    #[serde(rename = "continue")]
    #[builder(default = true)]
    should_continue: bool,

    /// The message shown to the user when `should_continue` is false.
    #[builder(into, default = "Nudge blocked operation due to rule violation")]
    stop_reason: String,

    /// Whether to hide the output of the hook from the user.
    #[builder(default = false)]
    suppress_output: bool,

    /// Message to display to the user when this response is emitted.
    #[builder(into)]
    system_message: String,

    /// Hook specific output.
    hook_specific_output: Option<T>,
}

/// Hook specific output for `PreToolUse` hooks.
#[derive(Debug, Serialize, Clone, Builder)]
#[serde(rename_all = "camelCase")]
struct PreToolUseHookSpecificOutput {
    /// The hook event name.
    #[builder(skip = String::from("PreToolUse"))]
    hook_event_name: String,

    /// The permission decision.
    #[builder(skip = String::from("deny"))]
    permission_decision: String,

    /// The reason for the decision being made, displayed to Claude Code.
    #[builder(into)]
    permission_decision_reason: String,
}

/// Configures a hook in Claude Code's settings.
#[derive(Debug, Serialize, Clone, Builder)]
#[non_exhaustive]
pub struct Config {
    /// The type of hook to run.
    ///
    /// Valid options are `command` or `prompt`, but we always want `command`:
    /// `prompt` hooks are run inside of Claude Code itself and cannot be
    /// intercepted by Nudge.
    #[builder(skip = String::from("command"))]
    pub r#type: String,

    /// The command to run.
    #[builder(into)]
    pub command: String,

    /// Terminate the command after this many seconds.
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

/// Evaluate all matchers in a given content and return the spans of all matches,
/// if and only if all the matchers matched the content.
///
/// If any matcher did not match the content, an empty vector is returned.
fn evaluate_all_matched(content: &str, matchers: &[ContentMatcher]) -> Vec<Span> {
    let mut spans = Vec::new();
    for matcher in matchers {
        let matches = matcher.matches(content);
        if matches.is_empty() {
            return Vec::new();
        }
        spans.extend(matches);
    }
    spans
}
