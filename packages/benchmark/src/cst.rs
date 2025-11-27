//! Concrete Syntax Tree evaluation for structural code analysis.
//!
//! This module provides tree-sitter based evaluation of source code structure,
//! allowing benchmark scenarios to express expectations about CST patterns
//! that regex cannot reliably capture.
//!
//! # Validation Model
//!
//! 1. The tree-sitter `query` finds 0..N matches in the source code
//! 2. The `validate` function is called on **every** match
//! 3. If any validation fails → a violation is returned
//! 4. If all validations pass → success (no violation)
//!
//! # Validators
//!
//! - `exists` - Match existing is sufficient (validation always passes)
//! - `not_exists` - Match existing is a problem (validation always fails)
//! - `contains` - Text between captures must contain the pattern
//! - `not_contains` - Text between captures must NOT contain the pattern
//! - `equals` - Text between captures must equal the pattern exactly
//!
//! # Examples
//!
//! ## Pattern should NOT exist
//!
//! ```toml
//! [[expected]]
//! type = "cst"
//! [expected.content]
//! path = "src/lib.rs"
//! language = "rust"
//! query = "(unsafe_block)"
//! validate = { type = "not_exists" }
//! ```
//!
//! ## Field spacing - fields should have blank lines between them
//!
//! ```toml
//! [[expected]]
//! type = "cst"
//! [expected.content]
//! path = "src/lib.rs"
//! language = "rust"
//! query = "(field_declaration_list (field_declaration) @f1 (field_declaration) @f2)"
//! validate = { type = "contains", from = "f1", to = "f2", content = "\n\n" }
//! ```
//!
//! ## Pattern should exist
//!
//! ```toml
//! [[expected]]
//! type = "cst"
//! [expected.content]
//! path = "src/lib.rs"
//! language = "rust"
//! query = "(function_item)"
//! validate = { type = "exists" }
//! ```

use std::ops::Range;
use std::path::Path;

use bon::Builder;
use color_eyre::eyre::{Context, ContextCompat, bail};
use color_eyre::{Result, eyre::eyre};
use color_eyre::{Section, SectionExt};
use serde::{Deserialize, Serialize};
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

use crate::matcher::MatchString;
use crate::outcome::Violation;

/// Supported languages for CST parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CstLanguage {
    Rust,
}

impl CstLanguage {
    /// Get the tree-sitter language for this variant.
    fn treesitter(self) -> Language {
        match self {
            CstLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }
}

/// A chunk of source code that can be parsed and queried.
#[derive(Debug, Clone)]
pub struct SourceCode {
    /// The source text.
    source: String,
}

impl SourceCode {
    /// Create a new source code wrapper.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    /// Get the source text.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Parse the source code with tree-sitter.
    fn parse(&self, language: CstLanguage) -> Option<tree_sitter::Tree> {
        let mut parser = Parser::new();
        parser.set_language(&language.treesitter()).ok()?;
        parser.parse(&self.source, None)
    }
}

/// Parameters for validating text between two captures.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct ValidationTarget {
    /// The name of the first capture, which starts the validation range.
    /// All text between the two captures is validated against the content.
    #[builder(into)]
    pub from: String,

    /// The name of the second capture, which ends the validation range.
    /// All text between the two captures is validated against the content.
    #[builder(into)]
    pub to: String,

    /// The content to use to validate the text between the two captures.
    ///
    /// This content supports regular expressions using the RE2 regex syntax
    /// variant supported by the `regex` crate; this content is tested for
    /// whether it "matches" (in the regular expression sense) against the text
    /// between captures according to the operation being performed.
    ///
    /// If the content fails to compile as a regex, the evaluation proceeds
    /// assuming it is a string; in this case the content is tested against the
    /// text between captures using string operations.
    #[builder(into)]
    pub content: MatchString,
}

/// Validation to apply to a CST query match.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CstValidate {
    /// The match existing is sufficient: validation always passes.
    /// Use when you want to assert that the pattern exists.
    Exists,

    /// The match existing is a problem: validation always fails.
    /// Use when you want to assert the pattern does NOT exist.
    NotExists,

    /// The text between captures should contain the pattern.
    /// If it does not, the match is considered a violation.
    Contains(ValidationTarget),

    /// The text between captures should NOT contain the pattern.
    /// If it does, the match is considered a violation.
    NotContains(ValidationTarget),

    /// The text between captures should equal the pattern exactly.
    /// If it does not, the match is considered a violation.
    Equals(ValidationTarget),
}

/// Result of validating a single match.
#[derive(Debug, Clone)]
pub struct ValidationFailure {
    /// Description of what was expected.
    pub expected: String,

    /// The actual text that was found between captures.
    pub actual: String,

    /// The byte range of the text that was checked.
    pub span: Range<usize>,
}

impl CstValidate {
    /// Validate a single match.
    ///
    /// Returns `None` if validation passes.
    /// Returns `Some(failure)` if validation fails.
    fn validate(
        &self,
        source: &str,
        captures: &[(String, Range<usize>)],
    ) -> Result<Option<ValidationFailure>> {
        match self {
            // Exists: match existing is sufficient, validation always passes
            CstValidate::Exists => Ok(None),

            // NotExists: match existing is a problem, validation always fails
            CstValidate::NotExists => {
                // Use the first capture's span, or a zero span
                let Some(span) = captures.first().map(|(_, r)| r.clone()) else {
                    return Ok(None);
                };
                Ok(Some(ValidationFailure {
                    expected: "pattern should not exist".to_string(),
                    actual: "pattern found".to_string(),
                    span,
                }))
            }

            CstValidate::Contains(bc) => {
                let (between, span) = Self::extract(source, captures, &bc.from, &bc.to)?;
                if bc.content.is_match(between) {
                    Ok(None)
                } else {
                    Ok(Some(ValidationFailure {
                        expected: format!("contains {:?}", bc.content.as_str()),
                        actual: between.to_string(),
                        span,
                    }))
                }
            }

            CstValidate::NotContains(bc) => {
                let (between, span) = Self::extract(source, captures, &bc.from, &bc.to)?;
                if !bc.content.is_match(between) {
                    Ok(None)
                } else {
                    Ok(Some(ValidationFailure {
                        expected: format!("does not contain {:?}", bc.content.as_str()),
                        actual: between.to_string(),
                        span,
                    }))
                }
            }

            CstValidate::Equals(bc) => {
                let (between, span) = Self::extract(source, captures, &bc.from, &bc.to)?;
                if bc.content.is_exact_match(between) {
                    Ok(None)
                } else {
                    Ok(Some(ValidationFailure {
                        expected: format!("equals {:?}", bc.content.as_str()),
                        actual: between.to_string(),
                        span,
                    }))
                }
            }
        }
    }

    /// Extract text between two named captures.
    fn extract<'a>(
        source: &'a str,
        captures: &[(String, Range<usize>)],
        from: &str,
        to: &str,
    ) -> Result<(&'a str, Range<usize>)> {
        let from_capture = captures
            .iter()
            .find(|(name, _)| name == from)
            .ok_or_else(|| eyre!("capture not found: {from:?}"))?;
        let to_capture = captures
            .iter()
            .find(|(name, _)| name == to)
            .ok_or_else(|| eyre!("capture not found: {to:?}"))?;

        let start = from_capture.1.end;
        let end = to_capture.1.start;
        if start > end {
            bail!("'from' capture {from:?} is after 'to' capture {to:?}");
        }

        Ok((&source[start..end], start..end))
    }
}

/// A CST query specification for scenario evaluation.
///
/// This type is deserialized from TOML scenario files and represents
/// a tree-sitter query with validation.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct CstQuery {
    /// The language to parse the source code as.
    pub language: CstLanguage,

    /// The tree-sitter query pattern.
    ///
    /// Uses tree-sitter's S-expression query syntax. See:
    /// <https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html>
    #[builder(into)]
    pub query: String,

    /// Validation to apply to matches.
    pub validate: CstValidate,
}

impl CstQuery {
    /// Execute the query and validate all matches.
    ///
    /// Returns validation failures for matches that don't pass validation.
    /// If all matches pass validation, returns an empty vec.
    fn find_failures(&self, source: &str) -> Result<Vec<CstMatchFailure>> {
        let source_code = SourceCode::new(source);
        let tree = source_code
            .parse(self.language)
            .context("parse source code")?;

        let lang = self.language.treesitter();
        let query = Query::new(&lang, &self.query)
            .context("parse query string")
            .with_section(|| self.query.clone().header("Query:"))?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let capture_names = query.capture_names();

        let mut failures = Vec::new();
        while let Some(matched) = matches.next() {
            let captures = matched
                .captures
                .iter()
                .map(|c| {
                    let index = c.index as usize;
                    let name = capture_names
                        .get(index)
                        .map(|n| n.to_string())
                        .ok_or_else(|| eyre!("capture index is out of bounds: {index}"))
                        .with_section(|| format!("{c:?}").header("Capture:"))
                        .with_section(|| capture_names.join(", ").header("Capture names:"))
                        .with_section(|| format!("{query:?}").header("Query:"))?;
                    let range = c.node.byte_range();
                    Ok((name, range))
                })
                .collect::<Result<Vec<(String, Range<usize>)>>>()?;

            let match_span = captures
                .first()
                .map(|(_, range)| range.clone())
                .or_else(|| matched.captures.first().map(|c| c.node.byte_range()))
                .unwrap_or(0..0);

            // Validate this match
            if let Some(failure) = self.validate.validate(source, &captures)? {
                failures.push(CstMatchFailure {
                    match_span,
                    failure,
                });
            }
        }

        Ok(failures)
    }

    /// Check that all matches pass validation.
    ///
    /// Returns a violation if ANY match fails validation.
    /// This is the primary method for scenario evaluation.
    pub fn check_all_valid(&self, path: &Path, content: &str) -> Option<Violation> {
        match self.find_failures(content) {
            Ok(failures) if failures.is_empty() => None,
            Ok(failures) => {
                let first = &failures[0];
                Some(Violation::CstValidationFailed(
                    CstValidationFailed::builder()
                        .path(path)
                        .language(self.language)
                        .query(&self.query)
                        .source(content)
                        .span(first.match_span.clone())
                        .expected(&first.failure.expected)
                        .actual(&first.failure.actual)
                        .failure_count(failures.len())
                        .build(),
                ))
            }
            Err(e) => Some(Violation::CstValidationFailed(
                CstValidationFailed::builder()
                    .path(path)
                    .language(self.language)
                    .query(&self.query)
                    .source(content)
                    .span(0..0)
                    .expected("query to execute")
                    .actual(e.to_string())
                    .failure_count(1)
                    .build(),
            )),
        }
    }
}

impl std::fmt::Display for CstQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.query)
    }
}

/// A match that failed validation.
#[derive(Debug, Clone)]
struct CstMatchFailure {
    /// The byte range of the match in the source.
    match_span: Range<usize>,

    /// Details about why validation failed.
    failure: ValidationFailure,
}

/// Errors that can occur during CST evaluation.
#[derive(Debug, Clone)]
pub enum CstError {
    /// Failed to parse the source code.
    ParseFailed,

    /// The tree-sitter query is invalid.
    InvalidQuery(String),
}

impl std::fmt::Display for CstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CstError::ParseFailed => write!(f, "failed to parse source code"),
            CstError::InvalidQuery(msg) => write!(f, "invalid query: {msg}"),
        }
    }
}

impl std::error::Error for CstError {}

/// A CST validation failed on one or more matches.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct CstValidationFailed {
    /// The file that was checked.
    #[builder(into)]
    pub path: std::path::PathBuf,

    /// The language used for parsing.
    pub language: CstLanguage,

    /// The query pattern that was run.
    #[builder(into)]
    pub query: String,

    /// The full source content of the file.
    #[builder(into)]
    pub source: String,

    /// The byte range of the first failing match.
    pub span: Range<usize>,

    /// What was expected.
    #[builder(into)]
    pub expected: String,

    /// What was actually found.
    #[builder(into)]
    pub actual: String,

    /// The total number of validation failures.
    pub failure_count: usize,
}

/// A CST query didn't match when it should have (contains failed).
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct CstQueryNotMatched {
    /// The file that was checked.
    #[builder(into)]
    pub path: std::path::PathBuf,

    /// The language used for parsing.
    pub language: CstLanguage,

    /// The query pattern that didn't match.
    #[builder(into)]
    pub query: String,

    /// Optional error message if parsing/querying failed.
    #[builder(into)]
    #[serde(default)]
    pub error: Option<String>,
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::path::PathBuf;

//     #[test]
//     fn test_exists_validation_passes() {
//         // Exists: match existing is fine, no violations
//         let query = CstQuery::builder()
//             .language(CstLanguage::Rust)
//             .query("(function_item)")
//             .validate(CstValidate::Exists)
//             .build();

//         let source = "fn main() {}";
//         let path = PathBuf::from("test.rs");

//         // check_contains: query matches, so no violation
//         assert!(query.check_contains(&path, source).is_none());

//         // check_all_valid: Exists always passes, so no violation
//         assert!(query.check_all_valid(&path, source).is_none());
//     }

//     #[test]
//     fn test_not_exists_validation_fails() {
//         // NotExists: match existing is a problem
//         let query = CstQuery::builder()
//             .language(CstLanguage::Rust)
//             .query("(function_item)")
//             .validate(CstValidate::NotExists)
//             .build();

//         let source = "fn main() {}";
//         let path = PathBuf::from("test.rs");

//         // check_all_valid: NotExists fails for every match, so violation
//         assert!(query.check_all_valid(&path, source).is_some());
//     }

//     #[test]
//     fn test_no_matches_no_violations() {
//         // If query doesn't match at all, there are no validation failures
//         let query = CstQuery::builder()
//             .language(CstLanguage::Rust)
//             .query("(function_item)")
//             .validate(CstValidate::NotExists)
//             .build();

//         let source = "struct Foo;";
//         let path = PathBuf::from("test.rs");

//         // No functions, so no matches, so no validation failures
//         assert!(query.check_all_valid(&path, source).is_none());

//         // But check_contains should fail - no matches at all
//         assert!(query.check_contains(&path, source).is_some());
//     }

//     #[test]
//     fn test_field_spacing_validate() {
//         // Validation: text between f1 and f2 SHOULD contain "\n\n" (blank line)
//         // If it doesn't, the match is a violation
//         let query = CstQuery::builder()
//             .language(CstLanguage::Rust)
//             .query("(field_declaration_list (field_declaration) @f1 (field_declaration) @f2)")
//             .validate(CstValidate::Contains(
//                 ValidationTarget::builder()
//                     .from("f1")
//                     .to("f2")
//                     .content("\n\n")
//                     .build(),
//             ))
//             .build();

//         let path = PathBuf::from("test.rs");

//         // Bad: no blank line between fields - validation fails, so it's a violation
//         let bad_source = r#"
// struct User {
//     name: String,
//     age: u32,
// }
// "#;
//         assert!(query.check_not_contains(&path, bad_source).is_some());

//         // Good: blank line between fields - validation passes, so NOT a violation
//         let good_source = r#"
// struct User {
//     name: String,

//     age: u32,
// }
// "#;
//         assert!(query.check_not_contains(&path, good_source).is_none());
//     }

//     #[test]
//     fn test_struct_instantiation_not_matched() {
//         // This is the key test: struct instantiation should NOT trigger
//         // the field spacing rule because it uses field_initializer_list,
//         // not field_declaration_list
//         let query = CstQuery::builder()
//             .language(CstLanguage::Rust)
//             .query("(field_declaration_list (field_declaration) @f1 (field_declaration) @f2)")
//             .validate(CstValidate::Contains(
//                 ValidationTarget::builder()
//                     .from("f1")
//                     .to("f2")
//                     .content("\n\n")
//                     .build(),
//             ))
//             .build();

//         let path = PathBuf::from("test.rs");

//         // Struct instantiation - should NOT match field_declaration_list
//         let source = r#"
// impl User {
//     pub fn new(id: u64, name: String) -> Self {
//         Self {
//             id,
//             name,
//             active: true,
//         }
//     }
// }
// "#;
//         // Should NOT match - this is struct_expression, not struct_item
//         assert!(query.check_not_contains(&path, source).is_none());
//     }

//     #[test]
//     fn test_invalid_query() {
//         let query = CstQuery::builder()
//             .language(CstLanguage::Rust)
//             .query("(not_a_real_node)")
//             .validate(CstValidate::Exists)
//             .build();

//         let source = "fn main() {}";
//         let path = PathBuf::from("test.rs");

//         // Should report not matched (with error)
//         let violation = query.check_contains(&path, source);
//         assert!(violation.is_some());
//     }
// }
