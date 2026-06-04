use std::{
    io::Write,
    process::{Command, Stdio},
    sync::LazyLock,
};

use derive_more::Display;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tree_sitter::{QueryCursor, StreamingIterator};

use crate::{
    snippet::{Match, Span},
    template::{self, Captures},
};

use super::{Language, TreeSitterQuery, syntax::union_of_captures};

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

        /// Optional replacement template for substitution rules.
        ///
        /// Replacement templates use the same capture interpolation syntax as
        /// suggestions, such as `{{ $1 }}` and `{{ $name }}`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replace: Option<String>,

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
    /// precise pattern matching that regex cannot achieve.
    SyntaxTree {
        /// The language grammar to use for parsing.
        language: Language,

        /// The tree-sitter query pattern.
        query: TreeSitterQuery,

        /// Optional suggestion template, same as Regex variant.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suggestion: Option<String>,
    },

    /// Match using an external program.
    ///
    /// Runs the specified command with the content piped to stdin. If the
    /// command exits with a non-zero status, the rule matches.
    External {
        /// The command to run, as a list of arguments.
        command: Vec<String>,
    },
}

impl<'de> Deserialize<'de> for ContentMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            kind: String,
            pattern: Option<String>,
            language: Option<Language>,
            query: Option<String>,
            command: Option<Vec<String>>,
            suggestion: Option<String>,
            replace: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;

        match raw.kind.as_str() {
            "Regex" => Ok(ContentMatcher::Regex {
                pattern: deserialize_regex(raw.pattern)?,
                replace: raw.replace,
                suggestion: raw.suggestion,
            }),
            "SyntaxTree" => {
                let language = raw
                    .language
                    .ok_or_else(|| serde::de::Error::missing_field("language"))?;
                let query = raw
                    .query
                    .ok_or_else(|| serde::de::Error::missing_field("query"))
                    .and_then(|query| {
                        TreeSitterQuery::new(language, query).map_err(serde::de::Error::custom)
                    })?;
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
            } => syntax_tree_matches(*language, query, s)
                .into_iter()
                .next()
                .is_some(),
            ContentMatcher::External { command } => run_external_command(command, s).is_some(),
        }
    }

    /// Get the spans of all matches in a given string.
    pub fn matches(&self, s: &str) -> Vec<Span> {
        match self {
            ContentMatcher::Regex { pattern, .. } => pattern.matches(s),
            ContentMatcher::SyntaxTree {
                language, query, ..
            } => syntax_tree_matches(*language, query, s)
                .into_iter()
                .map(|m| m.span)
                .collect(),
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
    pub fn matches_with_context(&self, s: &str) -> Vec<Match> {
        match self {
            ContentMatcher::Regex {
                pattern,
                suggestion,
                ..
            } => {
                let mut matches = pattern.matches_with_context(s);
                apply_suggestion(&mut matches, suggestion);
                matches
            }
            ContentMatcher::SyntaxTree {
                language,
                query,
                suggestion,
            } => {
                let mut matches = syntax_tree_matches(*language, query, s);
                apply_suggestion(&mut matches, suggestion);
                matches
            }
            ContentMatcher::External { command } => {
                if let Some(command) = run_external_command(command, s) {
                    let captures = Captures::from_iter([(String::from("command"), command)]);
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

    /// Apply this matcher's replacement template to a string.
    ///
    /// Only regex matchers support mechanical substitution. Other matcher
    /// types can still participate in gating a substitute rule, but they do not
    /// rewrite content.
    pub fn replace_all(&self, s: &str) -> String {
        match self {
            ContentMatcher::Regex {
                pattern,
                replace: Some(template),
                ..
            } => pattern.replace_all(s, template),
            _ => s.to_string(),
        }
    }

    /// Whether this matcher can change content for a substitution rule.
    pub fn has_replacement(&self) -> bool {
        matches!(
            self,
            ContentMatcher::Regex {
                replace: Some(_),
                ..
            }
        )
    }
}

fn syntax_tree_matches(language: Language, query: &TreeSitterQuery, source: &str) -> Vec<Match> {
    let Some(tree) = language.parse(source) else {
        return Vec::new();
    };

    let mut cursor = QueryCursor::new();
    let capture_names = query.as_ref().capture_names();
    let mut ts_matches = cursor.matches(query.as_ref(), tree.root_node(), source.as_bytes());

    let mut matches = Vec::new();
    while let Some(m) = ts_matches.next() {
        let span = union_of_captures(m);
        let mut captures = Captures::new();

        for capture in m.captures {
            if let Some(name) = capture_names.get(capture.index as usize) {
                let text = capture
                    .node
                    .utf8_text(source.as_bytes())
                    .unwrap_or_default()
                    .to_string();
                captures.insert(name.to_string(), text);
            }
        }

        matches.push(Match { span, captures });
    }

    matches
}

pub(crate) fn apply_suggestion(matches: &mut [Match], suggestion: &Option<String>) {
    let Some(suggestion_template) = suggestion else {
        return;
    };

    for m in matches {
        let interpolated = template::interpolate(suggestion_template, &m.captures);
        m.captures.insert(String::from("suggestion"), interpolated);
    }
}

pub(crate) fn deserialize_regex<E>(pattern: Option<String>) -> Result<RegexMatcher, E>
where
    E: serde::de::Error,
{
    let pattern = pattern.ok_or_else(|| serde::de::Error::missing_field("pattern"))?;
    Regex::new(&pattern)
        .map(RegexMatcher)
        .map_err(serde::de::Error::custom)
}

/// Run an external command with content piped to stdin.
///
/// Returns `Some(formatted_command)` if the command exits with non-zero status
/// (indicating a match/violation), or `None` if the command succeeds.
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

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(content.as_bytes());
    }

    match child.wait() {
        Ok(status) if status.success() => None,
        Ok(_) => Some(shell_words::join(command)),
        Err(e) => {
            tracing::warn!(?program, error = %e, "failed to wait for external command");
            None
        }
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
    pub fn matches(&self, s: &str) -> Vec<Span> {
        self.0.find_iter(s).map(|m| m.range().into()).collect()
    }

    /// Get matches with capture groups for template interpolation.
    pub fn matches_with_context(&self, s: &str) -> Vec<Match> {
        self.0
            .captures_iter(s)
            .map(|caps| {
                let full_match = caps.get(0).expect("capture 0 always exists");
                let span = Span::from(full_match.range());
                let captures = self.captures(&caps);

                Match { span, captures }
            })
            .collect()
    }

    pub fn replace_all(&self, s: &str, replacement_template: &str) -> String {
        self.0
            .replace_all(s, |caps: &regex::Captures<'_>| {
                let captures = self.captures(caps);
                template::interpolate(replacement_template, &captures)
            })
            .into_owned()
    }

    fn captures(&self, caps: &regex::Captures<'_>) -> Captures {
        let mut captures = Captures::new();

        for i in 0..caps.len() {
            if let Some(cap) = caps.get(i) {
                captures.insert(i.to_string(), cap.as_str().to_string());
            }
        }

        for name in self.0.capture_names().flatten() {
            if let Some(cap) = caps.name(name) {
                captures.insert(name.to_string(), cap.as_str().to_string());
            }
        }

        captures
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
        Regex::new(&s)
            .map(RegexMatcher)
            .map_err(serde::de::Error::custom)
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

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use crate::rules::{Language, TreeSitterQuery};

    use super::*;

    #[test]
    fn test_content_matcher_syntax_tree_deserialize() {
        let yaml = r#"
            kind: SyntaxTree
            language: rust
            query: "(function_item)"
        "#;
        let matcher = serde_yaml::from_str::<ContentMatcher>(yaml).expect("valid yaml");
        assert!(matches!(matcher, ContentMatcher::SyntaxTree { .. }));
    }

    #[test]
    fn test_content_matcher_syntax_tree_deserialize_haskell() {
        let yaml = r#"
            kind: SyntaxTree
            language: haskell
            query: |
              (apply
                function: (variable) @fn
                (#eq? @fn "head"))
        "#;
        let matcher = serde_yaml::from_str::<ContentMatcher>(yaml).expect("valid haskell yaml");
        assert!(matches!(
            matcher,
            ContentMatcher::SyntaxTree {
                language: Language::Haskell,
                ..
            }
        ));
    }

    #[test]
    fn regex_replace_interpolates_captures() {
        let matcher = serde_yaml::from_str::<ContentMatcher>(
            r#"
            kind: Regex
            pattern: "^npm install(?: (?P<args>.*))?$"
            replace: "yarn add {{ $args }}"
        "#,
        )
        .expect("valid regex matcher yaml");

        pretty_assert_eq!(matcher.replace_all("npm install lodash"), "yarn add lodash");
    }

    #[test]
    fn test_content_matcher_syntax_tree_deserialize_invalid_query() {
        let yaml = r#"
            kind: SyntaxTree
            language: rust
            query: "(not_a_real_node)"
        "#;
        let result = serde_yaml::from_str::<ContentMatcher>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_content_matcher_syntax_tree_deserialize_invalid_haskell_query() {
        let yaml = r#"
            kind: SyntaxTree
            language: haskell
            query: "(not_a_real_node)"
        "#;
        let result = serde_yaml::from_str::<ContentMatcher>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_syntax_tree_is_match() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(Language::Rust, "(function_item)").expect("valid query"),
            suggestion: None,
        };
        assert!(matcher.is_match("fn foo() {}"));
        assert!(!matcher.is_match("let x = 1;"));
    }

    #[test]
    fn test_syntax_tree_matches_returns_spans() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(Language::Rust, "(function_item) @fn")
                .expect("valid tree-sitter query"),
            suggestion: None,
        };
        let code = "fn foo() {}\nfn bar() {}";
        let spans = matcher.matches(code);
        pretty_assert_eq!(spans.len(), 2);
    }

    #[test]
    fn test_syntax_tree_captures_as_source_text() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(
                Language::Rust,
                "(function_item body: (block (use_declaration) @use))",
            )
            .expect("valid tree-sitter query"),
            suggestion: None,
        };

        let code_match = "fn foo() { use std::io; }";
        assert!(matcher.is_match(code_match));

        let code_no_match = "use std::io;\nfn foo() {}";
        assert!(!matcher.is_match(code_no_match));
    }

    #[test]
    fn test_syntax_tree_malformed_code_passes() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(Language::Rust, "(function_item)")
                .expect("valid tree-sitter query"),
            suggestion: None,
        };
        let malformed = "{{{{";
        assert!(!matcher.is_match(malformed));
    }

    #[test]
    fn test_union_of_captures_multiple() {
        let matcher = ContentMatcher::SyntaxTree {
            language: Language::Rust,
            query: TreeSitterQuery::new(
                Language::Rust,
                "(function_item name: (identifier) @name body: (block) @body)",
            )
            .expect("valid tree-sitter query"),
            suggestion: None,
        };
        let code = "fn foo() { let x = 1; }";
        let spans = matcher.matches(code);
        pretty_assert_eq!(spans.len(), 1);
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
        let matcher =
            serde_yaml::from_str::<ContentMatcher>(yaml).expect("valid external matcher yaml");
        assert!(matches!(matcher, ContentMatcher::External { .. }));
    }

    #[test]
    fn test_external_deserialize_empty_command() {
        let yaml = r#"
            kind: External
            command: []
        "#;
        let result = serde_yaml::from_str::<ContentMatcher>(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_external_is_match_when_command_fails() {
        let matcher = ContentMatcher::External {
            command: vec![String::from("false")],
        };
        assert!(matcher.is_match("any content"));
    }

    #[test]
    fn test_external_is_not_match_when_command_succeeds() {
        let matcher = ContentMatcher::External {
            command: vec![String::from("true")],
        };
        assert!(!matcher.is_match("any content"));
    }

    #[test]
    fn test_external_matches_with_context_sets_command_capture() {
        let matcher = ContentMatcher::External {
            command: vec![String::from("false")],
        };
        let matches = matcher.matches_with_context("content");
        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(
            matches[0].captures.get("command"),
            Some(&String::from("false"))
        );
    }

    #[test]
    fn test_external_matches_with_context_formats_command_with_args() {
        let matcher = ContentMatcher::External {
            command: vec![
                String::from("test"),
                String::from("1"),
                String::from("-eq"),
                String::from("0"),
            ],
        };
        let matches = matcher.matches_with_context("content");
        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(
            matches[0].captures.get("command"),
            Some(&String::from("test 1 -eq 0"))
        );
    }

    #[test]
    fn test_external_passes_content_to_stdin() {
        let matcher = ContentMatcher::External {
            command: vec![
                String::from("grep"),
                String::from("-q"),
                String::from("needle"),
            ],
        };
        assert!(!matcher.is_match("haystack with needle inside"));
        assert!(matcher.is_match("haystack without the search term"));
    }
}
