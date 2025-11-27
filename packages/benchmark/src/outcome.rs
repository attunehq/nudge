//! Structured outcomes for benchmark evaluations.
//!
//! This module provides types for representing the results of benchmark runs
//! in a structured way that supports:
//! - Nice CLI rendering
//! - JSON serialization for reports
//! - Aggregation across multiple runs

use std::ops::Range;
use std::path::PathBuf;

use bon::Builder;
use color_print::cformat;
use serde::{Deserialize, Serialize};

use crate::cst::{CstQueryNotMatched, CstValidationFailed};
use crate::snippet::Snippet;

/// The outcome of a single benchmark evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    /// All expectations passed.
    Pass,

    /// One or more expectations failed.
    Fail {
        /// The violations that caused the failure.
        violations: Vec<Violation>,
    },
}

impl Outcome {
    /// Create a passing outcome.
    pub fn pass() -> Self {
        Self::Pass
    }

    /// Create a failing outcome with violations.
    pub fn fail(violations: Vec<Violation>) -> Self {
        Self::Fail { violations }
    }

    /// Returns true if this outcome is a pass.
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Returns true if this outcome is a failure.
    pub fn is_fail(&self) -> bool {
        matches!(self, Self::Fail { .. })
    }

    /// Returns the violations if this is a failure, or an empty slice if it's a pass.
    pub fn violations(&self) -> &[Violation] {
        match self {
            Self::Pass => &[],
            Self::Fail { violations } => violations,
        }
    }
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "{}", cformat!("<green>✓</> Passed")),
            Self::Fail { violations } => {
                writeln!(f, "{}", cformat!("<red>✗</> Failed"))?;
                for violation in violations {
                    write!(f, "{violation}")?;
                }
                Ok(())
            }
        }
    }
}

/// A regex pattern matched when it shouldn't have (not_contains failed).
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct RegexMatched {
    /// The file where the match was found.
    #[builder(into)]
    pub path: PathBuf,

    /// The regex pattern that matched.
    #[builder(into)]
    pub pattern: String,

    /// The full source content of the file.
    #[builder(into)]
    pub source: String,

    /// The byte range of the match within the source.
    pub span: Range<usize>,
}

/// A regex pattern didn't match when it should have (contains failed).
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct RegexNotMatched {
    /// The file that was checked.
    #[builder(into)]
    pub path: PathBuf,

    /// The regex pattern that didn't match.
    #[builder(into)]
    pub pattern: String,
}

/// A string was found when it shouldn't have been (not_contains failed).
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct StringFound {
    /// The file where the string was found.
    #[builder(into)]
    pub path: PathBuf,

    /// The string that was found.
    #[builder(into)]
    pub needle: String,

    /// The full source content of the file.
    #[builder(into)]
    pub source: String,

    /// The byte range of the match within the source.
    pub span: Range<usize>,
}

/// A string wasn't found when it should have been (contains failed).
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct StringNotFound {
    /// The file that was checked.
    #[builder(into)]
    pub path: PathBuf,

    /// The string that wasn't found.
    #[builder(into)]
    pub needle: String,
}

/// File content didn't match expected content exactly (equals failed).
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct ContentMismatch {
    /// The file that was checked.
    #[builder(into)]
    pub path: PathBuf,

    /// Description of what was expected.
    #[builder(into)]
    pub expected: String,
}

/// A command failed during evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct CommandFailed {
    /// The command that failed.
    #[builder(into)]
    pub command: String,

    /// The exit code, if available.
    pub exit_code: Option<i32>,

    /// Stderr output, if any.
    #[builder(into)]
    pub stderr: Option<String>,
}

/// A specific violation that caused an expectation to fail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Violation {
    /// A regex pattern matched when it shouldn't have (not_contains failed).
    RegexMatched(RegexMatched),

    /// A regex pattern didn't match when it should have (contains failed).
    RegexNotMatched(RegexNotMatched),

    /// A string was found when it shouldn't have been (not_contains failed).
    StringFound(StringFound),

    /// A string wasn't found when it should have been (contains failed).
    StringNotFound(StringNotFound),

    /// File content didn't match expected content exactly (equals failed).
    ContentMismatch(ContentMismatch),

    /// A command failed during evaluation.
    CommandFailed(CommandFailed),

    /// A CST validation failed on one or more matches.
    CstValidationFailed(CstValidationFailed),

    /// A CST query didn't match when it should have (contains failed).
    CstQueryNotMatched(CstQueryNotMatched),
}


impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegexMatched(matched) => {
                let snippet = Snippet::new(&matched.source)
                    .render()
                    .highlight(matched.span.clone())
                    .finish()
                    .to_string();
                let pattern = &matched.pattern;
                let path = matched.path.display();
                writeln!(f, "{}", cformat!("<yellow>regex matched:</> <dim>{pattern}</>"))?;
                writeln!(f, "{}", cformat!("<dim>--></> {path}"))?;
                write!(f, "{snippet}")
            }
            Self::RegexNotMatched(not_matched) => {
                let pattern = &not_matched.pattern;
                let path = not_matched.path.display();
                writeln!(f, "{}", cformat!("<yellow>regex not matched:</> <dim>{pattern}</>"))?;
                write!(f, "{}", cformat!("<dim>--></> {path}"))
            }
            Self::StringFound(found) => {
                let snippet = Snippet::new(&found.source)
                    .render()
                    .highlight(found.span.clone())
                    .finish()
                    .to_string();
                let needle = &found.needle;
                let path = found.path.display();
                writeln!(f, "{}", cformat!("<yellow>string found:</> <dim>{needle:?}</>"))?;
                writeln!(f, "{}", cformat!("<dim>--></> {path}"))?;
                write!(f, "{snippet}")
            }
            Self::StringNotFound(not_found) => {
                let needle = &not_found.needle;
                let path = not_found.path.display();
                writeln!(f, "{}", cformat!("<yellow>string not found:</> <dim>{needle:?}</>"))?;
                write!(f, "{}", cformat!("<dim>--></> {path}"))
            }
            Self::ContentMismatch(mismatch) => {
                let expected = &mismatch.expected;
                let path = mismatch.path.display();
                writeln!(f, "{}", cformat!("<yellow>content mismatch:</> <dim>{expected}</>"))?;
                write!(f, "{}", cformat!("<dim>--></> {path}"))
            }
            Self::CommandFailed(failed) => {
                let command = &failed.command;
                write!(f, "{}", cformat!("<yellow>command failed:</> <dim>{command}</>"))?;
                if let Some(code) = failed.exit_code {
                    write!(f, " (exit code {code})")?;
                }
                if let Some(err) = &failed.stderr
                    && !err.is_empty()
                {
                    write!(f, "{}", cformat!("\n<dim>stderr:</> {err}"))?;
                }
                Ok(())
            }
            Self::CstValidationFailed(failed) => {
                let snippet = Snippet::new(&failed.source)
                    .render()
                    .highlight(failed.span.clone())
                    .finish()
                    .to_string();
                let query = &failed.query;
                let path = failed.path.display();
                let count = failed.failure_count;
                let expected = &failed.expected;
                let lang = format!("{:?}", failed.language).to_lowercase();
                writeln!(
                    f,
                    "{}",
                    cformat!("<yellow>cst validation failed:</> <dim>({lang}) {query}</>")
                )?;
                writeln!(f, "{}", cformat!("<dim>--></> {path}"))?;
                writeln!(
                    f,
                    "{}",
                    cformat!("<dim>expected:</> {expected} ({count} failure(s))")
                )?;
                write!(f, "{snippet}")
            }
            Self::CstQueryNotMatched(not_matched) => {
                let query = &not_matched.query;
                let path = not_matched.path.display();
                let lang = format!("{:?}", not_matched.language).to_lowercase();
                writeln!(
                    f,
                    "{}",
                    cformat!("<yellow>cst query not matched:</> <dim>({lang}) {query}</>")
                )?;
                write!(f, "{}", cformat!("<dim>--></> {path}"))?;
                if let Some(err) = &not_matched.error {
                    write!(f, "{}", cformat!("\n<dim>error:</> {err}"))?;
                }
                Ok(())
            }
        }
    }
}
