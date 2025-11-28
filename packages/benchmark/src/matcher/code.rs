use bon::Builder;
use color_eyre::{
    Result, Section, SectionExt,
    eyre::{Context, OptionExt},
};
use indoc::indoc;
use serde::{Deserialize, Deserializer, Serialize};
use serde_plain::derive_display_from_serialize;
use tree_sitter::{
    Language as TsLanguage, Parser as TsParser, Query as TsQuery, QueryCursor as TsQueryCursor,
    StreamingIterator as _, Tree as TsTree,
};

use crate::matcher::{FallibleMatcher, LabeledSpan, Match, Matches, Span};

/// Matches target code against a tree-sitter query.
#[derive(Debug, Clone, Builder)]
#[non_exhaustive]
pub struct CodeMatcher {
    /// The language of the code to match.
    pub language: Language,

    /// The query pattern to match.
    pub query: Query,
}

impl<'a> FallibleMatcher<&'a str> for CodeMatcher {
    fn find(&self, target: &str) -> Result<Matches> {
        let tree = self
            .language
            .parse_code(target)
            .context("parse code for match")?;

        let mut cursor = TsQueryCursor::new();
        let query = self.query.as_ref();
        let mut ts_matches = cursor.matches(query, tree.root_node(), target.as_bytes());

        // If the query has no capture names we should return unlabeled matches.
        match query.capture_names() {
            // Even when the query has no capture names, tree-sitter
            // still returns captures, these are just individual nodes
            // that were captured by the query.
            //
            // We want to report a union of all the spans reported by
            // treesitter- normally the first capture is the overall
            // match, but it's better to be sure.
            [] => {
                let mut matches = Vec::new();
                while let Some(matched) = ts_matches.next() {
                    let span = matched
                        .captures
                        .iter()
                        .map(|c| c.node.byte_range())
                        .fold((0, 0), |(start, end), range| {
                            (start.min(range.start), end.max(range.end))
                        });
                    matches.push(Span::from(span));
                }
                matches.sort_by_key(|m| m.start());
                Ok(Matches::Unlabeled(matches))
            }

            // When the query has capture names, treesitter returns captures
            // for both those names and also for the individual nodes that
            // were captured by the query.
            //
            // We want to report a union of all the spans reported by treesitter
            // for the overall match span, but we want to report each named
            // capture as a separate `LabeledSpan` inside.
            capture_names => {
                let mut matches = Vec::new();
                while let Some(matched) = ts_matches.next() {
                    let (captures, span) = matched.captures.iter().fold(
                        (Vec::new(), (0, 0)),
                        |(mut captures, (start, end)), c| {
                            let index = c.index as usize;
                            match capture_names.get(index).copied() {
                                Some(label) => {
                                    let range = c.node.byte_range();
                                    let span = LabeledSpan::new(label, range.clone());
                                    captures.push(span);
                                    (captures, (start.min(range.start), end.max(range.end)))
                                }
                                None => {
                                    let range = c.node.byte_range();
                                    (captures, (start.min(range.start), end.max(range.end)))
                                }
                            }
                        },
                    );
                    matches.push(Match::new(span, captures));
                }
                matches.sort_by_key(|m| m.span.start());
                Ok(Matches::Labeled(matches))
            }
        }
    }
}

impl<'de> Deserialize<'de> for CodeMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize to this first so that we can use the language to parse
        // the query, which treesitter requires.
        #[derive(Debug, Deserialize)]
        struct Intermediate {
            language: Language,
            query: String,
        }

        // Then we can parse the query with the language.
        let Intermediate { language, query } = Intermediate::deserialize(deserializer)?;
        let query = Query::parse(language, query).map_err(serde::de::Error::custom)?;
        Ok(CodeMatcher { language, query })
    }
}

/// The language of the code to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// The Rust programming language.
    Rust,
}

impl Language {
    /// Get the tree-sitter grammar for this language.
    pub fn treesitter(self) -> TsLanguage {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }

    /// Parse the given code into a syntax tree.
    pub fn parse_code(self, code: impl AsRef<str>) -> Result<TsTree> {
        let code = code.as_ref();
        let mut parser = TsParser::new();
        parser
            .set_language(&self.treesitter())
            .context("set language for parser")
            .with_section(|| self.to_string().header("Language:"))?;
        parser
            .parse(code, None)
            .ok_or_eyre("parse code")
            .with_section(|| self.to_string().header("Language:"))
            .with_section(|| code.to_string().header("Input:"))
    }
}

derive_display_from_serialize!(Language);

/// A query pattern used for matching code.
///
/// Uses tree-sitter's S-expression query syntax. Reference:
/// <https://tree-sitter.github.io/tree-sitter/using-parsers/queries/index.html>
#[derive(Debug)]
pub struct Query(TsQuery, String, Language);

impl Query {
    /// Parse a query string into a `Query` struct.
    pub fn parse(language: Language, query: impl Into<String>) -> Result<Self> {
        let query = query.into();
        TsQuery::new(&language.treesitter(), &query)
            .with_context(|| format!("parse query: {query:?}"))
            .with_section(|| query.to_string().header("Query:"))
            .with_section(|| language.to_string().header("Language:"))
            .suggestion(indoc! {"
                Query syntax is the same as the one used in tree-sitter queries.

                See the following documentation for more information:
                https://tree-sitter.github.io/tree-sitter/using-parsers/queries/index.html
            "})
            .map(|parsed| Self(parsed, query, language))
    }

    /// View the original query string that was parsed.
    pub fn as_str(&self) -> &str {
        &self.1
    }

    /// View the language that was used to parse the query.
    pub fn language(&self) -> Language {
        self.2
    }
}

impl Clone for Query {
    fn clone(&self) -> Self {
        Self::parse(self.2, &self.1).expect("query parsed before, it should parse again")
    }
}

impl AsRef<TsQuery> for Query {
    fn as_ref(&self) -> &TsQuery {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_matcher(query: &str) -> CodeMatcher {
        CodeMatcher {
            language: Language::Rust,
            query: Query::parse(Language::Rust, query).unwrap(),
        }
    }

    #[test]
    fn test_query_without_captures_returns_unlabeled() {
        let matcher = make_matcher("(function_item)");
        let code = "fn foo() {} fn bar() {}";
        let matches = matcher.find(code).unwrap();

        match matches {
            Matches::Unlabeled(spans) => {
                assert_eq!(spans.len(), 2);
                // First function
                assert_eq!(&code[spans[0].range()], "fn foo() {}");
                // Second function
                assert_eq!(&code[spans[1].range()], "fn bar() {}");
            }
            _ => panic!("expected Unlabeled, got {:?}", matches),
        }
    }

    #[test]
    fn test_query_with_captures_returns_labeled() {
        let matcher = make_matcher("(function_item name: (identifier) @name)");
        let code = "fn foo() {} fn bar() {}";
        let matches = matcher.find(code).unwrap();

        match matches {
            Matches::Labeled(ref ms) => {
                assert_eq!(ms.len(), 2);

                // First function
                let m0 = &ms[0];
                assert_eq!(&code[m0.span.range()], "foo");
                let name0 = m0.captures.iter().find(|c| c.label == "name").unwrap();
                assert_eq!(&code[name0.range()], "foo");

                // Second function
                let m1 = &ms[1];
                assert_eq!(&code[m1.span.range()], "bar");
                let name1 = m1.captures.iter().find(|c| c.label == "name").unwrap();
                assert_eq!(&code[name1.range()], "bar");
            }
            _ => panic!("expected Labeled, got {:?}", matches),
        }
    }

    #[test]
    fn test_query_with_multiple_captures() {
        let matcher = make_matcher("(function_item name: (identifier) @name body: (block) @body)");
        let code = "fn foo() { 1 }";
        let matches = matcher.find(code).unwrap();

        match matches {
            Matches::Labeled(ref ms) => {
                assert_eq!(ms.len(), 1);
                let m = &ms[0];

                let labels: Vec<_> = m.captures.iter().map(|c| c.label.as_str()).collect();
                assert!(labels.contains(&"name"), "missing 'name', got {:?}", labels);
                assert!(labels.contains(&"body"), "missing 'body', got {:?}", labels);

                let name = m.captures.iter().find(|c| c.label == "name").unwrap();
                assert_eq!(&code[name.range()], "foo");

                let body = m.captures.iter().find(|c| c.label == "body").unwrap();
                assert_eq!(&code[body.range()], "{ 1 }");
            }
            _ => panic!("expected Labeled, got {:?}", matches),
        }
    }

    #[test]
    fn test_no_matches() {
        let matcher = make_matcher("(struct_item)");
        let code = "fn foo() {}";
        let matches = matcher.find(code).unwrap();

        // Should be empty
        assert!(matches.is_empty());
    }

    #[test]
    fn test_struct_fields() {
        // This is a more realistic example - finding struct field declarations
        let matcher = make_matcher("(field_declaration name: (field_identifier) @field)");
        let code = r#"
struct User {
    name: String,
    age: u32,
}
"#;
        let matches = matcher.find(code).unwrap();

        match matches {
            Matches::Labeled(ref ms) => {
                assert_eq!(ms.len(), 2);

                let field0 = ms[0].captures.iter().find(|c| c.label == "field").unwrap();
                assert_eq!(&code[field0.range()], "name");

                let field1 = ms[1].captures.iter().find(|c| c.label == "field").unwrap();
                assert_eq!(&code[field1.range()], "age");
            }
            _ => panic!("expected Labeled, got {:?}", matches),
        }
    }
}
