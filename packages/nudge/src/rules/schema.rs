//! Schema types for user-defined rules.

use std::{path::Path, sync::LazyLock};

use derive_more::Display;
use glob::Pattern;
use monostate::MustBe;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    fmap_match,
    snippet::{Annotation, Span},
};

/// A rule configuration file.
#[derive(Debug, Clone, Deserialize)]
pub struct RuleConfig {
    /// The version of the rule configuration file.
    pub version: MustBe!(1),

    /// The rules defined in this file.
    pub rules: Vec<Rule>,
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
    pub message: String,

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

    /// Convenience method to filter hooks to `UserPromptSubmit`.
    pub fn hooks_userpromptsubmit(&self) -> impl Iterator<Item = &UserPromptSubmitMatcher> {
        self.on
            .iter()
            .filter_map(fmap_match!(Hook::UserPromptSubmit))
    }

    /// Annotate matching spans with the message from the rule.
    pub fn annotate_spans(
        &self,
        spans: impl IntoIterator<Item = impl Into<Span>>,
    ) -> impl Iterator<Item = Annotation> {
        spans.into_iter().map(|span| {
            Annotation::builder()
                .span(span)
                .label(&self.message)
                .build()
        })
    }
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
///           pattern: "(?m)^[ \\t]+use "
///     - hook: PreToolUse
///       tool: Edit
///       file: "**/*.rs"
///       new_content:
///         - kind: Regex
///           pattern: "(?m)^[ \\t]+use "
/// ```
///
/// We have three options to handle this:
/// - Change the shape of the config file (not ideal as it's less ergonomic)
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

/// The method used to match hook content.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", content = "pattern")]
pub enum ContentMatcher {
    /// Match on a regular expression.
    Regex(RegexMatcher),
}

impl ContentMatcher {
    /// Test whether this pattern matches a given string.
    pub fn is_match(&self, s: &str) -> bool {
        match self {
            ContentMatcher::Regex(matcher) => matcher.is_match(s),
        }
    }

    /// Get the spans of all matches in a given string.
    ///
    /// Spans are returned in the order of the matches, and are non-overlapping.
    pub fn matches(&self, s: &str) -> Vec<Span> {
        match self {
            ContentMatcher::Regex(matcher) => matcher.matches(s),
        }
    }
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
}

/// Match on a glob pattern.
#[derive(Debug, Clone, Display)]
#[display("{_0}")]
pub struct GlobMatcher(Pattern);

impl GlobMatcher {
    /// Create an instance that matches any string.
    pub fn any() -> Self {
        static ANY: LazyLock<Pattern> =
            LazyLock::new(|| Pattern::new("**/*").expect("compile 'any' glob pattern"));
        GlobMatcher(ANY.clone())
    }

    /// Test whether this pattern matches a given string.
    pub fn is_match(&self, path: &str) -> bool {
        self.0.matches(path)
    }

    /// Test whether this pattern matches the given path.
    pub fn is_match_path(&self, path: &Path) -> bool {
        self.0.matches_path(path)
    }
}

impl Default for GlobMatcher {
    fn default() -> Self {
        Self::any()
    }
}

impl<'de> Deserialize<'de> for GlobMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let pattern = Pattern::new(&s).map_err(serde::de::Error::custom)?;
        Ok(GlobMatcher(pattern))
    }
}

impl Serialize for GlobMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

/// Match on a regex pattern.
#[derive(Debug, Clone, Display)]
#[display("{_0}")]
pub struct RegexMatcher(Regex);

impl RegexMatcher {
    /// Create an instance that matches any string.
    pub fn any() -> Self {
        static ANY: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(".*").expect("compile 'any' regex"));
        RegexMatcher(ANY.clone())
    }

    /// Create an instance that matches nothing.
    pub fn none() -> Self {
        static NONE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new("a^").expect("compile 'none' regex"));
        RegexMatcher(NONE.clone())
    }

    /// Test whether this pattern matches a given string.
    pub fn is_match(&self, s: &str) -> bool {
        self.0.is_match(s)
    }

    /// Get the spans of all matches in a given string.
    ///
    /// Spans are returned in the order of the matches, and are non-overlapping.
    pub fn matches(&self, s: &str) -> Vec<Span> {
        self.0.find_iter(s).map(|m| m.range().into()).collect()
    }
}

impl Default for RegexMatcher {
    fn default() -> Self {
        Self::any()
    }
}

impl<'de> Deserialize<'de> for RegexMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let regex = Regex::new(&s).map_err(serde::de::Error::custom)?;
        Ok(RegexMatcher(regex))
    }
}

impl Serialize for RegexMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}
