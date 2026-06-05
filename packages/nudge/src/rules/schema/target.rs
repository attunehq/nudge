//! File-content target selection for Write/Edit rules.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::{snippet::Match, template::Captures};

use super::{ContentMatcher, Language};

/// The part of a matched file that content matchers evaluate.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum FileContentTarget {
    /// Evaluate matchers against the raw file content.
    #[default]
    Content,

    /// Evaluate matchers against fenced Markdown code blocks for one language.
    MarkdownCodeBlock {
        /// The language named by the fenced code block info string.
        language: Language,
    },
}

impl FileContentTarget {
    /// Evaluate all matchers against this target and return translated matches.
    pub fn evaluate(&self, content: &str, matchers: &[ContentMatcher]) -> Vec<Match> {
        match self {
            FileContentTarget::Content => evaluate_all_matched(content, matchers),
            FileContentTarget::MarkdownCodeBlock { language } => markdown_code_blocks(content)
                .into_iter()
                .enumerate()
                .filter(|(_, block)| language.matches_markdown_info_word(&block.language))
                .flat_map(|(index, block)| {
                    let mut matches = evaluate_all_matched(block.source, matchers);
                    for m in &mut matches {
                        m.span.start += block.body_start;
                        m.span.end += block.body_start;
                        m.captures.extend(Captures::from_iter([
                            (String::from("markdown_language"), block.language.clone()),
                            (String::from("markdown_info"), block.info.clone()),
                            (
                                String::from("markdown_block_start_line"),
                                block.start_line.to_string(),
                            ),
                            (String::from("markdown_block_index"), index.to_string()),
                        ]));
                    }
                    matches
                })
                .collect(),
        }
    }
}

/// Evaluate all content matchers and return matches only if every matcher
/// matched the same target string.
pub fn evaluate_all_matched(content: &str, matchers: &[ContentMatcher]) -> Vec<Match> {
    let mut matches = Vec::new();
    for matcher in matchers {
        let matcher_matches = matcher.matches_with_context(content);
        if matcher_matches.is_empty() {
            return Vec::new();
        }
        matches.extend(matcher_matches);
    }
    matches
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkdownCodeBlock<'a> {
    source: &'a str,
    body_start: usize,
    language: String,
    info: String,
    start_line: usize,
}

fn markdown_code_blocks(markdown: &str) -> Vec<MarkdownCodeBlock<'_>> {
    let mut blocks = Vec::new();
    let mut current = None::<OpenCodeBlock>;

    for (event, range) in Parser::new_ext(markdown, Options::empty()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let info = info.trim().to_string();
                let language = info
                    .split_whitespace()
                    .next()
                    .map(normalize_markdown_language)
                    .unwrap_or_default();
                current = Some(OpenCodeBlock {
                    info,
                    language,
                    body_start: range.end,
                    body_end: range.end,
                    start_line: byte_offset_to_line(markdown, range.start),
                });
            }
            Event::Text(_) => {
                if let Some(block) = &mut current {
                    block.body_start = block.body_start.min(range.start);
                    block.body_end = block.body_end.max(range.end);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(block) = current.take()
                    && !block.language.is_empty()
                    && block.body_start <= block.body_end
                    && block.body_end <= markdown.len()
                {
                    blocks.push(MarkdownCodeBlock {
                        source: &markdown[block.body_start..block.body_end],
                        body_start: block.body_start,
                        language: block.language,
                        info: block.info,
                        start_line: block.start_line,
                    });
                }
            }
            _ => {}
        }
    }

    blocks
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenCodeBlock {
    info: String,
    language: String,
    body_start: usize,
    body_end: usize,
    start_line: usize,
}

fn normalize_markdown_language(word: &str) -> String {
    word.trim()
        .trim_start_matches("{.")
        .trim_start_matches('.')
        .trim_end_matches('}')
        .to_ascii_lowercase()
}

fn byte_offset_to_line(content: &str, offset: usize) -> usize {
    content[..offset.min(content.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1
}

impl Language {
    fn matches_markdown_info_word(self, word: &str) -> bool {
        matches!(
            (self, word),
            (Language::Rust, "rust" | "rs")
                | (Language::TypeScript, "typescript" | "ts" | "tsx")
                | (Language::JavaScript, "javascript" | "js" | "jsx")
                | (Language::Python, "python" | "py")
                | (Language::Go, "go" | "golang")
                | (Language::Java, "java")
                | (Language::CSharp, "csharp" | "c-sharp" | "cs")
                | (Language::Kotlin, "kotlin" | "kt" | "kts")
                | (Language::Haskell, "haskell" | "hs")
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{rules::TreeSitterQuery, snippet::Span};
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::*;

    fn regex(pattern: &str) -> ContentMatcher {
        serde_yaml::from_str(&format!(
            r#"
kind: Regex
pattern: "{pattern}"
"#
        ))
        .expect("valid regex matcher")
    }

    #[test]
    fn content_target_preserves_raw_content_matching() {
        let target = FileContentTarget::Content;
        let matches = target.evaluate("one TODO\n", &[regex("TODO")]);

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(matches[0].span, Span { start: 4, end: 8 });
    }

    #[test]
    fn markdown_code_block_target_matches_inside_one_fence() {
        let target = FileContentTarget::MarkdownCodeBlock {
            language: Language::Rust,
        };
        let markdown = "# Example\n\n```rust\nfn main() {\n    let value: usize = 1;\n}\n```\n";
        let matches = target.evaluate(
            markdown,
            &[ContentMatcher::SyntaxTree {
                language: Language::Rust,
                query: TreeSitterQuery::new(
                    Language::Rust,
                    "(let_declaration pattern: (identifier) @binding type: (_) @type)",
                )
                .expect("valid query"),
                suggestion: None,
            }],
        );

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(matches[0].captures["binding"], "value");
        pretty_assert_eq!(matches[0].captures["markdown_language"], "rust");
        pretty_assert_eq!(matches[0].captures["markdown_block_start_line"], "3");
        pretty_assert_eq!(
            &markdown[matches[0].span.start..matches[0].span.end],
            "value: usize"
        );
    }

    #[test]
    fn markdown_code_block_target_requires_all_matchers_in_same_fence() {
        let target = FileContentTarget::MarkdownCodeBlock {
            language: Language::Rust,
        };
        let markdown = "```rust\nfn first() {}\n```\n\n```rust\nlet second: usize = 1;\n```\n";
        let matches = target.evaluate(markdown, &[regex("first"), regex("second")]);

        assert!(matches.is_empty());
    }

    #[test]
    fn markdown_code_block_target_respects_language_aliases() {
        let target = FileContentTarget::MarkdownCodeBlock {
            language: Language::TypeScript,
        };
        let markdown = "```tsx\nvalue!\n```\n";
        let matches = target.evaluate(markdown, &[regex("value!")]);

        pretty_assert_eq!(matches.len(), 1);
    }
}
