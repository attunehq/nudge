//! Schema types for user-defined rules.

use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::LazyLock,
};

use derive_more::Display;
use glob::Pattern;
use monostate::MustBe;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tree_sitter::{
    Language as TsLanguage, Parser, Query, QueryCursor, QueryError, StreamingIterator,
};

use crate::{
    fmap_match,
    snippet::{Annotation, Match, Span},
    template::{self, Captures},
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
            let label = template::interpolate(&self.message, &m.captures);
            Annotation::builder().span(m.span).label(label).build()
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

/// The method used to match hook content.
///
/// Uses custom deserialization because the `SyntaxTree` variant needs to
/// compile tree-sitter queries at parse time, which requires the `language`
/// field to be available when processing the `query` field.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum ContentMatcher {
    /// Match on a regular expression.
    Regex {
        /// The regex pattern to match.
        pattern: RegexMatcher,

        /// Optional suggestion template for this matcher.
        ///
        /// When provided, the suggestion is interpolated with the match's
        /// capture groups and added to the match context as `suggestion`.
        /// This can then be referenced in the rule's message as `{{ $suggestion
        /// }}`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suggestion: Option<String>,
    },

    /// Match on a tree-sitter syntax query.
    ///
    /// Tree-sitter queries match against the AST structure of code, enabling
    /// precise pattern matching that regex cannot achieve (e.g., matching
    /// `use` statements only inside function bodies, not in `mod test` blocks).
    ///
    /// Query syntax: <https://tree-sitter.github.io/tree-sitter/using-parsers/queries>
    SyntaxTree {
        /// The language grammar to use for parsing.
        ///
        /// Required because tree-sitter queries must be compiled against a
        /// specific grammar, and we validate queries at rule load time rather
        /// than deferring to match time.
        language: Language,

        /// The tree-sitter query pattern.
        query: TreeSitterQuery,

        /// Optional suggestion template, same as Regex variant.
        ///
        /// Captures from the query (e.g., `@fn_name`) can be referenced as
        /// `{{ $fn_name }}` in the suggestion template.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suggestion: Option<String>,
    },

    /// Match using an external program.
    ///
    /// Runs the specified command with the content piped to stdin. If the
    /// command exits with a non-zero status, the rule matches.
    ///
    /// This enables integration with external linters and formatters like
    /// `markdownlint`, `prettier --check`, etc.
    External {
        /// The command to run, as a list of arguments.
        ///
        /// The first element is the program, subsequent elements are arguments.
        /// The content being checked is piped to the command's stdin.
        ///
        /// Example: `["npx", "markdownlint", "--stdin"]`
        command: Vec<String>,
    },
}

impl<'de> Deserialize<'de> for ContentMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Intermediate representation that can deserialize all variants.
        // We use raw strings for fields that need post-processing.
        #[derive(Deserialize)]
        struct Raw {
            kind: String,
            // Regex fields
            pattern: Option<String>,
            // SyntaxTree fields
            language: Option<Language>,
            query: Option<String>,
            // External fields
            command: Option<Vec<String>>,
            // Shared field
            suggestion: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;

        match raw.kind.as_str() {
            "Regex" => {
                let pattern_str = raw
                    .pattern
                    .ok_or_else(|| serde::de::Error::missing_field("pattern"))?;
                let pattern = Regex::new(&pattern_str)
                    .map(RegexMatcher)
                    .map_err(serde::de::Error::custom)?;
                Ok(ContentMatcher::Regex {
                    pattern,
                    suggestion: raw.suggestion,
                })
            }
            "SyntaxTree" => {
                let language = raw
                    .language
                    .ok_or_else(|| serde::de::Error::missing_field("language"))?;
                let query_str = raw
                    .query
                    .ok_or_else(|| serde::de::Error::missing_field("query"))?;
                let query =
                    TreeSitterQuery::new(language, query_str).map_err(serde::de::Error::custom)?;
                Ok(ContentMatcher::SyntaxTree {
                    language,
                    query,
                    suggestion: raw.suggestion,
                })
            }
            "External" => {
                let command = raw
                    .command
                    .ok_or_else(|| serde::de::Error::missing_field("command"))?;
                if command.is_empty() {
                    return Err(serde::de::Error::custom("command cannot be empty"));
                }
                Ok(ContentMatcher::External { command })
            }
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["Regex", "SyntaxTree", "External"],
            )),
        }
    }
}

impl ContentMatcher {
    /// Test whether this pattern matches a given string.
    pub fn is_match(&self, s: &str) -> bool {
        match self {
            ContentMatcher::Regex { pattern, .. } => pattern.is_match(s),
            ContentMatcher::SyntaxTree {
                language, query, ..
            } => {
                let Some(tree) = language.parse(s) else {
                    return false;
                };
                let mut cursor = QueryCursor::new();
                let mut matches = cursor.matches(query.as_ref(), tree.root_node(), s.as_bytes());
                matches.next().is_some()
            }
            ContentMatcher::External { command } => run_external_command(command, s).is_some(),
        }
    }

    /// Get the spans of all matches in a given string.
    ///
    /// Spans are returned in the order of the matches, and are non-overlapping.
    ///
    /// `External` matchers are arbitrary programs that don't return span
    /// context, so any such "match" returns a span covering the entire content.
    pub fn matches(&self, s: &str) -> Vec<Span> {
        match self {
            ContentMatcher::Regex { pattern, .. } => pattern.matches(s),
            ContentMatcher::SyntaxTree {
                language, query, ..
            } => {
                let Some(tree) = language.parse(s) else {
                    return Vec::new();
                };
                let mut cursor = QueryCursor::new();
                let mut ts_matches = cursor.matches(query.as_ref(), tree.root_node(), s.as_bytes());
                let mut spans = Vec::new();
                while let Some(m) = ts_matches.next() {
                    spans.push(union_of_captures(m));
                }
                spans
            }
            ContentMatcher::External { command } => {
                if run_external_command(command, s).is_some() {
                    vec![Span::from(0..s.len())]
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// Get matches with capture groups for template interpolation.
    ///
    /// If this matcher has a suggestion template, it will be interpolated
    /// with the match's captures and added to the context as `suggestion`.
    ///
    /// `External` matchers are arbitrary programs that don't return span
    /// context, so any such "match" returns a span covering the entire content.
    pub fn matches_with_context(&self, s: &str) -> Vec<Match> {
        match self {
            ContentMatcher::Regex {
                pattern,
                suggestion,
            } => {
                let mut matches = pattern.matches_with_context(s);

                // If there's a suggestion template, interpolate it per-match
                if let Some(suggestion_template) = suggestion {
                    for m in &mut matches {
                        let interpolated = template::interpolate(suggestion_template, &m.captures);
                        m.captures.insert("suggestion".to_string(), interpolated);
                    }
                }

                matches
            }
            ContentMatcher::SyntaxTree {
                language,
                query,
                suggestion,
            } => {
                let Some(tree) = language.parse(s) else {
                    return Vec::new();
                };

                let mut cursor = QueryCursor::new();
                let capture_names = query.as_ref().capture_names();
                let mut ts_matches = cursor.matches(query.as_ref(), tree.root_node(), s.as_bytes());

                let mut matches = Vec::new();
                while let Some(m) = ts_matches.next() {
                    let span = union_of_captures(m);
                    let mut captures = Captures::new();

                    // Extract named captures as source text, matching regex behavior.
                    // This allows suggestions to reference captures like {{ $fn_name }}.
                    for capture in m.captures {
                        if let Some(name) = capture_names.get(capture.index as usize) {
                            let text = capture
                                .node
                                .utf8_text(s.as_bytes())
                                .unwrap_or_default()
                                .to_string();
                            captures.insert(name.to_string(), text);
                        }
                    }

                    matches.push(Match { span, captures });
                }

                if let Some(suggestion_template) = suggestion {
                    for m in &mut matches {
                        let interpolated = template::interpolate(suggestion_template, &m.captures);
                        m.captures.insert("suggestion".to_string(), interpolated);
                    }
                }

                matches
            }
            ContentMatcher::External { command } => {
                if let Some(command) = run_external_command(command, s) {
                    let captures = Captures::from_iter([("command".to_string(), command)]);
                    vec![Match {
                        span: Span::from(0..s.len()),
                        captures,
                    }]
                } else {
                    Vec::new()
                }
            }
        }
    }
}

/// The method used to match URLs.
///
/// Similar to [`ContentMatcher`] but only supports regex patterns, since URLs
/// are simple strings without syntax tree structure.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum UrlMatcher {
    /// Match on a regular expression.
    Regex {
        /// The regex pattern to match against the URL.
        pattern: RegexMatcher,

        /// Optional suggestion template for this matcher.
        ///
        /// When provided, the suggestion is interpolated with the match's
        /// capture groups and added to the match context as `suggestion`.
        /// This can then be referenced in the rule's message as `{{ $suggestion
        /// }}`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suggestion: Option<String>,
    },
}

impl<'de> Deserialize<'de> for UrlMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            kind: String,
            pattern: Option<String>,
            suggestion: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;

        match raw.kind.as_str() {
            "Regex" => {
                let pattern_str = raw
                    .pattern
                    .ok_or_else(|| serde::de::Error::missing_field("pattern"))?;
                let pattern = Regex::new(&pattern_str)
                    .map(RegexMatcher)
                    .map_err(serde::de::Error::custom)?;
                Ok(UrlMatcher::Regex {
                    pattern,
                    suggestion: raw.suggestion,
                })
            }
            other => Err(serde::de::Error::unknown_variant(other, &["Regex"])),
        }
    }
}

impl UrlMatcher {
    /// Test whether this pattern matches a given URL.
    pub fn is_match(&self, url: &str) -> bool {
        match self {
            UrlMatcher::Regex { pattern, .. } => pattern.is_match(url),
        }
    }

    /// Get the spans of all matches in a given URL.
    pub fn matches(&self, url: &str) -> Vec<Span> {
        match self {
            UrlMatcher::Regex { pattern, .. } => pattern.matches(url),
        }
    }

    /// Get matches with capture groups for template interpolation.
    ///
    /// If this matcher has a suggestion template, it will be interpolated
    /// with the match's captures and added to the context as `suggestion`.
    pub fn matches_with_context(&self, url: &str) -> Vec<Match> {
        match self {
            UrlMatcher::Regex {
                pattern,
                suggestion,
            } => {
                let mut matches = pattern.matches_with_context(url);

                // If there's a suggestion template, interpolate it per-match
                if let Some(suggestion_template) = suggestion {
                    for m in &mut matches {
                        let interpolated = template::interpolate(suggestion_template, &m.captures);
                        m.captures.insert("suggestion".to_string(), interpolated);
                    }
                }

                matches
            }
        }
    }
}

/// Matcher for project state conditions.
///
/// Project state matchers evaluate conditions about the project environment
/// (e.g., git state) rather than the content of the tool input. All project
/// state matchers in a rule must match for the rule to proceed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum ProjectStateMatcher {
    /// Match against git repository state.
    Git {
        /// Match against the current branch name.
        ///
        /// All branch matchers must match the current branch name for this
        /// git matcher to pass.
        #[serde(default)]
        branch: Vec<ContentMatcher>,
    },
}

impl<'de> Deserialize<'de> for ProjectStateMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            kind: String,
            #[serde(default)]
            branch: Vec<ContentMatcher>,
        }

        let raw = Raw::deserialize(deserializer)?;

        match raw.kind.as_str() {
            "Git" => Ok(ProjectStateMatcher::Git { branch: raw.branch }),
            other => Err(serde::de::Error::unknown_variant(other, &["Git"])),
        }
    }
}

impl ProjectStateMatcher {
    /// Evaluate this matcher against the project state at the given path.
    ///
    /// Returns `true` if the matcher matches, `false` otherwise.
    /// Logs a warning if the project state cannot be determined (e.g., not
    /// in a git repository).
    pub fn is_match(&self, cwd: &Path) -> bool {
        match self {
            ProjectStateMatcher::Git { branch } => {
                if branch.is_empty() {
                    // No branch matchers = always match (just checking we're in a git repo)
                    if crate::git::current_branch(cwd).is_some() {
                        return true;
                    }
                    tracing::warn!(?cwd, "project_state.Git matcher: not in a git repository");
                    return false;
                }

                let Some(current_branch) = crate::git::current_branch(cwd) else {
                    tracing::warn!(
                        ?cwd,
                        "project_state.Git matcher: could not determine current branch"
                    );
                    return false;
                };

                // All branch matchers must match
                branch.iter().all(|m| m.is_match(&current_branch))
            }
        }
    }
}

/// Run an external command with content piped to stdin.
///
/// Returns `Some(formatted_command)` if the command exits with non-zero status
/// (indicating a match/violation), or `None` if the command succeeds (no
/// violation).
fn run_external_command(command: &[String], content: &str) -> Option<String> {
    let Some((program, args)) = command.split_first() else {
        tracing::warn!("external command is empty");
        return None;
    };

    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            tracing::warn!(?program, error = %e, "failed to spawn external command");
            return None;
        }
    };

    // Write content to stdin
    if let Some(mut stdin) = child.stdin.take()
        && let Err(e) = stdin.write_all(content.as_bytes())
    {
        tracing::warn!(?program, error = %e, "failed to write to external command stdin");
        return None;
    }

    // Wait for the command to complete
    match child.wait() {
        Ok(status) => {
            if status.success() {
                // Command succeeded = no violation
                None
            } else {
                // Command failed = violation detected
                // Format the command for the hint message
                Some(shell_words::join(command))
            }
        }
        Err(e) => {
            tracing::warn!(?program, error = %e, "failed to wait for external command");
            None
        }
    }
}

/// Compute the union span of all captures in a tree-sitter match.
///
/// Tree-sitter matches can have multiple captures; we highlight the entire
/// matched region rather than individual captures for consistency with how
/// regex matches display the full match.
fn union_of_captures(m: &tree_sitter::QueryMatch) -> Span {
    if m.captures.is_empty() {
        // Empty captures shouldn't happen with valid queries that have @captures,
        // but return a placeholder span pointing to file start if it does.
        tracing::warn!("tree-sitter match has no captures");
        return Span { start: 0, end: 0 };
    }

    let (start, end) = m.captures.iter().fold((usize::MAX, 0), |(start, end), c| {
        let range = c.node.byte_range();
        (start.min(range.start), end.max(range.end))
    });

    // Defensive: ensure valid span even if captures have unexpected ranges
    if start > end {
        tracing::warn!("tree-sitter match produced invalid byte ranges");
        return Span { start: 0, end: 0 };
    }

    Span { start, end }
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

    /// Get matches with capture groups for template interpolation.
    ///
    /// Returns `Match` objects containing both the span and captured values.
    /// Captures are stored as:
    /// - `"0"`, `"1"`, `"2"`, etc. for positional captures
    /// - Named keys for named capture groups (e.g., `"var_name"`)
    pub fn matches_with_context(&self, s: &str) -> Vec<Match> {
        self.0
            .captures_iter(s)
            .map(|caps| {
                let full_match = caps.get(0).expect("capture 0 always exists");
                let span = Span::from(full_match.range());

                let mut captures = Captures::new();

                // Add positional captures
                for i in 0..caps.len() {
                    if let Some(cap) = caps.get(i) {
                        captures.insert(i.to_string(), cap.as_str().to_string());
                    }
                }

                // Add named captures
                for name in self.0.capture_names().flatten() {
                    if let Some(cap) = caps.name(name) {
                        captures.insert(name.to_string(), cap.as_str().to_string());
                    }
                }

                Match { span, captures }
            })
            .collect()
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

/// Supported languages for tree-sitter parsing.
///
/// Adding a new language requires:
/// 1. Adding the grammar crate to Cargo.toml (e.g., `tree-sitter-python`)
/// 2. Adding a variant to this enum
/// 3. Adding a match arm to `grammar()` that returns the language
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// The Rust programming language.
    Rust,
    /// The TypeScript programming language.
    TypeScript,
}

impl Language {
    /// Get the tree-sitter grammar for this language.
    pub fn grammar(self) -> TsLanguage {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        }
    }

    /// Parse source code into a syntax tree.
    ///
    /// Returns `None` if parsing fails (e.g., malformed code). We intentionally
    /// don't block on parse errors since code being written is often
    /// incomplete.
    pub fn parse(self, source: &str) -> Option<tree_sitter::Tree> {
        use std::sync::Mutex;

        // Reuse parsers across calls. Parser creation has non-trivial overhead,
        // and parsers are designed to be reused. We use Mutex because parsing
        // is stateful (the parser tracks incremental parse state).
        static RUST_PARSER: LazyLock<Mutex<Parser>> = LazyLock::new(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_rust::LANGUAGE.into())
                .expect("failed to set Rust language");
            Mutex::new(parser)
        });

        static TYPESCRIPT_PARSER: LazyLock<Mutex<Parser>> = LazyLock::new(|| {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
                .expect("failed to set TypeScript language");
            Mutex::new(parser)
        });

        let mut parser = match self {
            Language::Rust => RUST_PARSER.lock().ok()?,
            Language::TypeScript => TYPESCRIPT_PARSER.lock().ok()?,
        };

        let tree = parser.parse(source, None)?;

        // Log a warning if the tree has errors, but still return it.
        // Partial parses can still match valid subtrees.
        if tree.root_node().has_error() {
            tracing::debug!(language = ?self, "parsed code contains syntax errors");
        }

        Some(tree)
    }
}

/// A compiled tree-sitter query.
///
/// Wraps `tree_sitter::Query` with the original source and language for
/// serialization and cloning. Queries are compiled at deserialization time
/// to catch errors early (at rule load time, not match time).
#[derive(Debug)]
pub struct TreeSitterQuery {
    inner: Query,
    source: String,
    language: Language,
}

impl TreeSitterQuery {
    /// Compile a query from source for the given language.
    pub fn new(language: Language, source: impl Into<String>) -> Result<Self, QueryError> {
        let source = source.into();
        let inner = Query::new(&language.grammar(), &source)?;
        Ok(Self {
            inner,
            source,
            language,
        })
    }
}

impl AsRef<Query> for TreeSitterQuery {
    fn as_ref(&self) -> &Query {
        &self.inner
    }
}

impl Clone for TreeSitterQuery {
    fn clone(&self) -> Self {
        // Safe to unwrap: if it compiled once, it will compile again
        Self::new(self.language, &self.source).expect("query compiled before, should compile again")
    }
}

impl Serialize for TreeSitterQuery {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.source)
    }
}

/// Deserialize a tree-sitter query.
///
/// This requires special handling because tree-sitter queries must be compiled
/// against a language grammar. We use `deserialize_with` at the struct level
/// to access the sibling `language` field.
impl<'de> Deserialize<'de> for TreeSitterQuery {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // This impl exists only to satisfy the trait bound. Actual deserialization
        // happens via the custom deserializer for ContentMatcher::SyntaxTree.
        Err(serde::de::Error::custom(
            "TreeSitterQuery cannot be deserialized standalone; use ContentMatcher::SyntaxTree",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_parse_valid_rust() {
        let code = "fn main() { println!(\"hello\"); }";
        let tree = Language::Rust.parse(code);
        assert!(tree.is_some());
    }

    #[test]
    fn test_language_parse_invalid_returns_tree_with_errors() {
        // Tree-sitter is error-tolerant; it returns a tree even for invalid code
        let code = "fn main( { }";
        let tree = Language::Rust.parse(code);
        assert!(tree.is_some());
    }

    #[test]
    fn test_treesitter_query_compile_valid() {
        let query = TreeSitterQuery::new(Language::Rust, "(function_item)");
        assert!(query.is_ok());
    }

    #[test]
    fn test_treesitter_query_compile_invalid() {
        let query = TreeSitterQuery::new(Language::Rust, "(not_a_real_node)");
        assert!(query.is_err());
    }

    #[test]
    fn test_content_matcher_syntax_tree_deserialize() {
        let yaml = r#"
            kind: SyntaxTree
            language: rust
            query: "(function_item)"
        "#;
        let matcher: ContentMatcher = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(matcher, ContentMatcher::SyntaxTree { .. }));
    }

    #[test]
    fn test_content_matcher_syntax_tree_deserialize_invalid_query() {
        let yaml = r#"
            kind: SyntaxTree
            language: rust
            query: "(not_a_real_node)"
        "#;
        let result: Result<ContentMatcher, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_syntax_tree_is_match() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(Language::Rust, "(function_item)").unwrap(),
            suggestion: None,
        };
        assert!(matcher.is_match("fn foo() {}"));
        assert!(!matcher.is_match("let x = 1;"));
    }

    #[test]
    fn test_syntax_tree_matches_returns_spans() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(Language::Rust, "(function_item) @fn").unwrap(),
            suggestion: None,
        };
        let code = "fn foo() {}\nfn bar() {}";
        let spans = matcher.matches(code);
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn test_syntax_tree_captures_as_source_text() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(
                Language::Rust,
                "(function_item name: (identifier) @fn_name)",
            )
            .unwrap(),
            suggestion: None,
        };
        let code = "fn my_function() {}";
        let matches = matcher.matches_with_context(code);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].captures.get("fn_name"),
            Some(&"my_function".to_string())
        );
    }

    #[test]
    fn test_syntax_tree_suggestion_interpolation() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(
                Language::Rust,
                "(function_item name: (identifier) @fn_name)",
            )
            .unwrap(),
            suggestion: Some("Rename {{ $fn_name }} to something descriptive".to_string()),
        };
        let code = "fn x() {}";
        let matches = matcher.matches_with_context(code);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].captures.get("suggestion"),
            Some(&"Rename x to something descriptive".to_string())
        );
    }

    #[test]
    fn test_syntax_tree_use_in_function_body() {
        // This is the motivating use case: match `use` inside function bodies
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(
                Language::Rust,
                "(function_item body: (block (use_declaration) @use))",
            )
            .unwrap(),
            suggestion: None,
        };

        // Should match: use inside function
        let code_match = "fn foo() { use std::io; }";
        assert!(matcher.is_match(code_match));

        // Should NOT match: top-level use
        let code_no_match = "use std::io;\nfn foo() {}";
        assert!(!matcher.is_match(code_no_match));
    }

    #[test]
    fn test_syntax_tree_malformed_code_passes() {
        // Malformed code should not match (passes silently)
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(Language::Rust, "(function_item)").unwrap(),
            suggestion: None,
        };
        // Completely malformed - no function keyword
        let malformed = "{{{{";
        // Tree-sitter is error-tolerant, but this won't match function_item
        assert!(!matcher.is_match(malformed));
    }

    #[test]
    fn test_union_of_captures_multiple() {
        // Test that the union span covers all captures
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(
                Language::Rust,
                "(function_item name: (identifier) @name body: (block) @body)",
            )
            .unwrap(),
            suggestion: None,
        };
        let code = "fn foo() { let x = 1; }";
        let spans = matcher.matches(code);
        assert_eq!(spans.len(), 1);
        // The span should cover from "foo" through the end of the block
        let matched_text = &code[spans[0].start..spans[0].end];
        assert!(matched_text.contains("foo"));
        assert!(matched_text.contains("let x = 1"));
    }

    #[test]
    fn test_external_deserialize() {
        let yaml = r#"
            kind: External
            command: ["grep", "-q", "error"]
        "#;
        let matcher: ContentMatcher = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(matcher, ContentMatcher::External { .. }));
    }

    #[test]
    fn test_external_deserialize_empty_command() {
        let yaml = r#"
            kind: External
            command: []
        "#;
        let result: Result<ContentMatcher, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_external_is_match_when_command_fails() {
        // `false` is a command that always exits with code 1
        let matcher = ContentMatcher::External {
            command: vec!["false".to_string()],
        };
        assert!(matcher.is_match("any content"));
    }

    #[test]
    fn test_external_is_not_match_when_command_succeeds() {
        // `true` is a command that always exits with code 0
        let matcher = ContentMatcher::External {
            command: vec!["true".to_string()],
        };
        assert!(!matcher.is_match("any content"));
    }

    #[test]
    fn test_external_matches_with_context_sets_command_capture() {
        // `false` always fails
        let matcher = ContentMatcher::External {
            command: vec!["false".to_string()],
        };
        let matches = matcher.matches_with_context("content");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].captures.get("command"),
            Some(&"false".to_string())
        );
    }

    #[test]
    fn test_external_matches_with_context_formats_command_with_args() {
        // `test 1 -eq 0` always fails (1 != 0) and tests multi-arg formatting
        let matcher = ContentMatcher::External {
            command: vec![
                "test".to_string(),
                "1".to_string(),
                "-eq".to_string(),
                "0".to_string(),
            ],
        };
        let matches = matcher.matches_with_context("content");
        assert_eq!(matches.len(), 1);
        // shell_words::join formats the command
        assert_eq!(
            matches[0].captures.get("command"),
            Some(&"test 1 -eq 0".to_string())
        );
    }

    #[test]
    fn test_external_passes_content_to_stdin() {
        // Use grep to check that specific content is passed via stdin
        let matcher = ContentMatcher::External {
            command: vec!["grep".to_string(), "-q".to_string(), "needle".to_string()],
        };
        // grep -q exits 0 if pattern found, 1 if not
        // So if "needle" is in content, grep succeeds (no match)
        // If "needle" is NOT in content, grep fails (match)
        assert!(!matcher.is_match("haystack with needle inside"));
        assert!(matcher.is_match("haystack without the search term"));
    }

    #[test]
    fn test_bash_matcher_deserialize() {
        let yaml = r#"
            hook: PreToolUse
            tool: Bash
            command:
              - kind: Regex
                pattern: "git\\s+push"
        "#;
        let matcher: PreToolUseBashMatcher = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(matcher.command.len(), 1);
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
        let matcher: PreToolUseBashMatcher = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(matcher.command.len(), 1);
        assert_eq!(matcher.project_state.len(), 1);
    }

    #[test]
    fn test_project_state_git_deserialize() {
        let yaml = r#"
            kind: Git
            branch:
              - kind: Regex
                pattern: "^main$"
        "#;
        let matcher: ProjectStateMatcher = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(matcher, ProjectStateMatcher::Git { .. }));
    }

    #[test]
    fn test_project_state_git_empty_branch() {
        let yaml = r#"
            kind: Git
            branch: []
        "#;
        let matcher: ProjectStateMatcher = serde_yaml::from_str(yaml).unwrap();
        let ProjectStateMatcher::Git { branch } = matcher;
        assert!(branch.is_empty());
    }

    #[test]
    fn test_project_state_invalid_kind() {
        let yaml = r#"
            kind: InvalidKind
        "#;
        let result: Result<ProjectStateMatcher, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }
}
