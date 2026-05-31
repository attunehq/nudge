//! Schema types for user-defined rules.

use monostate::MustBe;
use serde::{Deserialize, Serialize};

use crate::{
    fmap_match,
    snippet::{Annotation, Match, Span},
    template,
};

pub use content::{ContentMatcher, RegexMatcher};
pub use path::GlobMatcher;
pub use project_state::ProjectStateMatcher;
pub use prompt::{DurationSeconds, FileChangeMatcher, PromptIntentMatcher};
pub use syntax::{Language, TreeSitterQuery};
pub use url::UrlMatcher;

mod content;
mod path;
mod project_state;
mod prompt;
mod rust_indexed_iteration;
mod stuttering;
mod syntax;
mod url;

/// A rule configuration file.
#[derive(Debug, Clone, Deserialize)]
pub struct RuleConfig {
    /// The version of the rule configuration file.
    pub version: MustBe!(1),

    /// The rules defined in this file.
    #[serde(default)]
    pub rules: Vec<Rule>,

    /// Workflow completion gates defined in this file.
    #[serde(default)]
    pub workflows: Vec<Workflow>,
}

/// A workflow completion gate.
///
/// Workflows are opt-in stop-time checks. A workflow activates when a matching
/// user prompt is submitted, stores the original prompt for the session, and
/// asks the agent to confirm all configured done criteria before `Stop` can
/// pass through.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Workflow {
    /// Unique identifier for this workflow.
    pub name: String,

    /// Human-readable description of this workflow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Prompt patterns that activate this workflow.
    #[serde(default)]
    pub prompt: Vec<ContentMatcher>,

    /// Completion criteria the agent must verify before stopping.
    #[serde(default)]
    pub done: Vec<String>,
}

impl Workflow {
    /// Returns whether this workflow should activate for a user prompt.
    pub fn matches_prompt(&self, prompt: &str) -> bool {
        self.prompt.iter().all(|matcher| matcher.is_match(prompt))
    }

    /// The exact line the agent must include to confirm completion.
    pub fn confirmation_token(&self) -> String {
        format!("NUDGE_WORKFLOW_COMPLETE: {}", self.name)
    }
}

/// A single rule definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    /// Unique identifier for this rule.
    pub name: String,

    /// Human-readable description of the rule.
    pub description: Option<String>,

    /// The message to provide to the agent when the rule matches.
    ///
    /// This message is displayed to the agent in context with the matched hook
    /// context. For example, if the rule matches on specific content in the
    /// code edited by a `PreToolUse` hook, the message is displayed to the
    /// agent in context with the code that was edited.
    ///
    /// If multiple hook events match, this message is displayed to the agent
    /// for every matching hook event, still in context of the matching content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// What to do when this rule matches.
    #[serde(default)]
    pub action: RuleAction,

    /// The criteria under which this rule matches.
    ///
    /// Every incoming hook event from the agent is evaluated against this list
    /// in the order in which they are defined; if any rule matches then the
    /// overall rule is considered to match.
    pub on: Vec<Hook>,
}

impl Rule {
    /// Convenience method to filter hooks to `PreToolUse::Write`.
    pub fn hooks_pretooluse_write(&self) -> impl Iterator<Item = &PreToolUseWriteMatcher> {
        self.on
            .iter()
            .filter_map(fmap_match!(Hook::PreToolUse))
            .filter_map(fmap_match!(PreToolUseMatcher::Write))
    }

    /// Convenience method to filter hooks to `PreToolUse::Edit`.
    pub fn hooks_pretooluse_edit(&self) -> impl Iterator<Item = &PreToolUseEditMatcher> {
        self.on
            .iter()
            .filter_map(fmap_match!(Hook::PreToolUse))
            .filter_map(fmap_match!(PreToolUseMatcher::Edit))
    }

    /// Convenience method to filter hooks to `PreToolUse::WebFetch`.
    pub fn hooks_pretooluse_webfetch(&self) -> impl Iterator<Item = &PreToolUseWebFetchMatcher> {
        self.on
            .iter()
            .filter_map(fmap_match!(Hook::PreToolUse))
            .filter_map(fmap_match!(PreToolUseMatcher::WebFetch))
    }

    /// Convenience method to filter hooks to `PreToolUse::Bash`.
    pub fn hooks_pretooluse_bash(&self) -> impl Iterator<Item = &PreToolUseBashMatcher> {
        self.on
            .iter()
            .filter_map(fmap_match!(Hook::PreToolUse))
            .filter_map(fmap_match!(PreToolUseMatcher::Bash))
    }

    /// Convenience method to filter hooks to `UserPromptSubmit`.
    pub fn hooks_userpromptsubmit(&self) -> impl Iterator<Item = &UserPromptSubmitMatcher> {
        self.on
            .iter()
            .filter_map(fmap_match!(Hook::UserPromptSubmit))
    }

    /// Whether this rule needs local interaction state.
    pub fn uses_interaction_state(&self) -> bool {
        self.hooks_userpromptsubmit()
            .any(UserPromptSubmitMatcher::uses_interaction_state)
    }

    /// Annotate matching spans with the message from the rule.
    pub fn annotate_spans(
        &self,
        spans: impl IntoIterator<Item = impl Into<Span>>,
    ) -> impl Iterator<Item = Annotation> {
        spans.into_iter().map(|span| {
            Annotation::builder()
                .span(span)
                .label(self.message())
                .build()
        })
    }

    /// Annotate matches with interpolated messages.
    ///
    /// Each match's captures are used to interpolate the rule's message.
    /// If the match contains a `suggestion` key in its captures, that
    /// can be referenced in the message as `{{ $suggestion }}`.
    pub fn annotate_matches(
        &self,
        matches: impl IntoIterator<Item = Match>,
    ) -> impl Iterator<Item = Annotation> {
        matches.into_iter().map(|m| {
            let label = template::interpolate(self.message(), &m.captures);
            Annotation::builder().span(m.span).label(label).build()
        })
    }

    /// Message text for blocking annotations.
    pub fn message(&self) -> &str {
        self.message
            .as_deref()
            .unwrap_or("Rule matched. Fix this issue and retry.")
    }
}

/// Rule action when a match is found.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    /// Block the operation and show the rule message.
    #[default]
    Block,

    /// Rewrite matching Bash commands and let the operation proceed.
    Substitute,
}

impl From<&Rule> for Rule {
    fn from(value: &Rule) -> Self {
        value.clone()
    }
}

/// Matches hook events, strongly typed for each kind of hook.
///
/// # Deserialization
///
/// We have multiple types coming out of the same object in this type.
///
/// In the below example, the `on` field is a list of [`Hook`] values, and we
/// want to parse the `Hook` enum with the `hook` field. But we _also_ want to
/// parse the inner `tool` field as [`PreToolUseMatcher`] or
/// [`UserPromptSubmitMatcher`], both of which then parse other fields in the
/// object differently (such as `content`, `new_content`, `prompt`, etc).
///
/// ```yaml
/// version: 1
///
/// rules:
/// # Catch `use` statements inside function bodies.
/// # Pattern: lines starting with horizontal whitespace (not newlines) followed by `use `
/// - name: no-inline-imports
///   description: Move imports to the top of the file
///   message: Move this `use` statement to the top of the file with other imports, then retry.
///   on:
///     - hook: PreToolUse
///       tool: Write
///       file: "**/*.rs"
///       content:
///         - kind: Regex
///           pattern: "(?m)^[ \t]+use "
///     - hook: PreToolUse
///       tool: Edit
///       file: "**/*.rs"
///       new_content:
///         - kind: Regex
///           pattern: "(?m)^[ \t]+use "
/// ```
///
/// We have three options to handle this:
/// - Change the shape of the config file (less ergonomic)
/// - Use a custom deserializer (more complex)
/// - Use `monostate` in combination with fields that are unused in the struct,
///   so that instead of trying to parse as a tagged union serde just tries all
///   options until one works.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Hook {
    /// Matches the PreToolUse hook.
    ///
    /// If this rule matches during a `PreToolUse` hook, the message is
    /// displayed to the agent in context with the code that was edited and the
    /// action is blocked. The agent is instructed to retry the operation after
    /// fixing the issues raised by the rule.
    PreToolUse(PreToolUseMatcher),

    /// Matches the UserPromptSubmit hook.
    ///
    /// If this rule matches during a `UserPromptSubmit` hook, the message is
    /// displayed to the agent after the user's prompt; the intention is to
    /// allow rules to provide additional guidance to the agent in specific
    /// scenarios.
    UserPromptSubmit(UserPromptSubmitMatcher),
}

/// Matches the PreToolUse hook.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PreToolUseMatcher {
    /// Matches the Write tool.
    Write(PreToolUseWriteMatcher),

    /// Matches the Edit tool.
    Edit(PreToolUseEditMatcher),

    /// Matches the WebFetch tool.
    WebFetch(PreToolUseWebFetchMatcher),

    /// Matches the Bash tool.
    Bash(PreToolUseBashMatcher),
}

/// Matches the Write tool.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreToolUseWriteMatcher {
    // Monostate fields to parse the overall object.
    hook: MustBe!("PreToolUse"),
    tool: MustBe!("Write"),

    /// Glob pattern for files to match.
    ///
    /// When the path of the file being written to by the agent matches this
    /// pattern, the rule is triggered.
    #[serde(default)]
    pub file: GlobMatcher,

    /// Regex patterns for content to match.
    ///
    /// When the content being written by the agent matches all of these
    /// patterns, the rule is triggered.
    #[serde(default)]
    pub content: Vec<ContentMatcher>,
}

/// Matches the Edit tool.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreToolUseEditMatcher {
    // Monostate fields to parse the overall object.
    hook: MustBe!("PreToolUse"),
    tool: MustBe!("Edit"),

    /// Glob pattern for files to match.
    ///
    /// When the path of the file being edited by the agent matches this
    /// pattern, the rule is triggered.
    #[serde(default)]
    pub file: GlobMatcher,

    /// Regex patterns for new content to match.
    ///
    /// When the new content being written by the agent matches all of these
    /// patterns, the rule is triggered.
    #[serde(default)]
    pub new_content: Vec<ContentMatcher>,
}

/// Matches the WebFetch tool.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreToolUseWebFetchMatcher {
    // Monostate fields to parse the overall object.
    hook: MustBe!("PreToolUse"),
    tool: MustBe!("WebFetch"),

    /// URL patterns to match.
    ///
    /// When the URL being fetched by the agent matches all of these
    /// patterns, the rule is triggered.
    #[serde(default)]
    pub url: Vec<UrlMatcher>,
}

/// Matches the Bash tool.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreToolUseBashMatcher {
    // Monostate fields to parse the overall object.
    hook: MustBe!("PreToolUse"),
    tool: MustBe!("Bash"),

    /// Command patterns to match.
    ///
    /// When the command being executed by the agent matches all of these
    /// patterns, the rule is triggered.
    #[serde(default)]
    pub command: Vec<ContentMatcher>,

    /// Project state matchers.
    ///
    /// When all project state matchers match, the rule proceeds to evaluate
    /// the command matchers. If any project state matcher does not match,
    /// the rule does not fire.
    #[serde(default)]
    pub project_state: Vec<ProjectStateMatcher>,
}

/// Matches the UserPromptSubmit hook.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserPromptSubmitMatcher {
    // Monostate field to parse the overall object.
    hook: MustBe!("UserPromptSubmit"),

    /// Regex patterns for user prompt to match.
    ///
    /// When the user's prompt submitted to the agent matches all of these
    /// patterns, the rule is triggered.
    #[serde(default)]
    pub prompt: Vec<ContentMatcher>,

    /// Example-based semantic intent matcher for the user prompt.
    ///
    /// This is local and deterministic: Nudge compares normalized prompt tokens
    /// to author-provided examples. It never calls an external model or
    /// service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<PromptIntentMatcher>,

    /// Only match after a recent project file change.
    ///
    /// File changes are recorded locally only for rules that opt into this
    /// field, and only matching file paths and timestamps are stored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after_file_change: Vec<FileChangeMatcher>,

    /// Suppress this prompt reminder until the cooldown has elapsed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown: Option<DurationSeconds>,

    /// Suppress repeats until another matching file change occurs.
    #[serde(default)]
    pub once_per_change: bool,
}

impl UserPromptSubmitMatcher {
    /// Whether this matcher needs local interaction state.
    pub fn uses_interaction_state(&self) -> bool {
        !self.after_file_change.is_empty() || self.cooldown.is_some()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::*;

    #[test]
    fn test_bash_matcher_deserialize() {
        let yaml = r#"
            hook: PreToolUse
            tool: Bash
            command:
              - kind: Regex
                pattern: "git\\s+push"
        "#;
        let matcher =
            serde_yaml::from_str::<PreToolUseBashMatcher>(yaml).expect("valid bash matcher yaml");
        pretty_assert_eq!(matcher.command.len(), 1);
        assert!(matcher.project_state.is_empty());
    }

    #[test]
    fn test_bash_matcher_with_project_state_deserialize() {
        let yaml = r#"
            hook: PreToolUse
            tool: Bash
            command:
              - kind: Regex
                pattern: "git\\s+push"
            project_state:
              - kind: Git
                branch:
                  - kind: Regex
                    pattern: "^main$"
        "#;
        let matcher =
            serde_yaml::from_str::<PreToolUseBashMatcher>(yaml).expect("valid bash matcher yaml");
        pretty_assert_eq!(matcher.command.len(), 1);
        pretty_assert_eq!(matcher.project_state.len(), 1);
    }

    #[test]
    fn test_user_prompt_semantic_matcher_deserialize() {
        let yaml = r#"
            hook: UserPromptSubmit
            intent:
              examples:
                - "let's test this"
                - "try running it"
            after_file_change:
              - file: "packages/hurry/src/**"
                within: "30m"
            once_per_change: true
            cooldown: "1h"
        "#;
        let matcher = serde_yaml::from_str::<UserPromptSubmitMatcher>(yaml)
            .expect("valid user prompt matcher yaml");
        assert!(matcher.intent.is_some());
        pretty_assert_eq!(matcher.after_file_change.len(), 1);
        assert!(matcher.once_per_change);
        assert!(matcher.uses_interaction_state());
    }
}
