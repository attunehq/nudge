//! Schema types for user-defined rules.

use monostate::MustBe;
use serde::Deserialize;

/// A rule configuration file.
#[derive(Debug, Clone, Deserialize)]
pub struct RuleConfig {
    /// The version of the rule configuration file.
    pub version: MustBe!(1),

    /// The rules defined in this file.
    pub rules: Vec<Rule>,
}

/// A single rule definition.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    /// Unique identifier for this rule.
    pub name: String,

    /// Human-readable description.
    pub description: Option<String>,

    /// When this rule activates.
    pub on: Activation,

    /// Content matching criteria.
    #[serde(default)]
    pub r#match: Match,

    /// What happens when the rule fires.
    pub action: Action,

    /// Message to display (supports template interpolation).
    pub message: String,
}

/// Activation criteria for a rule.
#[derive(Debug, Clone, Deserialize)]
pub struct Activation {
    /// Which hook event type triggers this rule.
    pub hook: HookType,

    /// Regex pattern for tool name (only for *ToolUse hooks).
    pub tool: Option<String>,

    /// Glob pattern for file path.
    pub file: Option<String>,
}

/// Hook event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum HookType {
    /// The hook is triggered before a tool is used.
    PreToolUse,

    /// The hook is triggered after a tool is used.
    PostToolUse,

    /// The hook is triggered when a user prompt is submitted.
    UserPromptSubmit,

    /// The hook is triggered when the assistant stops.
    Stop,
}

/// Content matching criteria.
///
/// All patterns are regular expressions. Use inline flags for modifiers:
/// - `(?i)` - case-insensitive matching
/// - `(?m)` - multiline mode (^ and $ match line boundaries)
/// - `(?s)` - dotall mode (. matches newlines)
/// - `(?x)` - verbose mode (ignore whitespace, allow comments)
///
/// Example: `(?m)^\\s+use ` matches `use` statements at line start with leading whitespace.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Match {
    /// Regex for Write tool content.
    pub content: Option<String>,

    /// Regex for Edit tool new_string.
    pub new_string: Option<String>,

    /// Regex for Edit tool old_string.
    pub old_string: Option<String>,

    /// Regex for UserPromptSubmit prompt.
    pub prompt: Option<String>,

    /// Regex for Stop assistant message.
    pub message: Option<String>,
}

/// Response type when a rule fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Block the operation.
    Interrupt,

    /// Allow but inject guidance.
    Continue,
}
