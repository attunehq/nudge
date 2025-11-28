//! Structured outcomes for benchmark evaluations.
//!
//! This module provides types for representing the results of benchmark runs
//! in a structured way that supports:
//! - Nice CLI rendering
//! - JSON serialization for reports
//! - Aggregation across multiple runs

use std::fmt::{Display, Formatter};
use std::path::PathBuf;

use bon::Builder;
use color_print::{cformat, cwriteln};
use serde::{Deserialize, Serialize};

use crate::ext::indent;
use crate::matcher::Matches;
use crate::matcher::code::Language;
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

    /// Stdout output, if any.
    #[builder(into)]
    pub stdout: Option<String>,
}

impl Display for CommandFailed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let command = &self.command;
        let exit_code = self.exit_code.unwrap_or_default();
        let stderr = self.stderr.as_deref().unwrap_or("<none>");
        let stdout = self.stdout.as_deref().unwrap_or("<none>");
        cwriteln!(f, "<yellow>command failed:</> <dim>{command}</>")?;
        cwriteln!(f, "<cyan>-</> <yellow>exit code:</> {exit_code}")?;
        cwriteln!(f, "<cyan>-</> <yellow>stderr:</>")?;
        cwriteln!(f, "<dim>{}</>", stderr.indent(2))?;
        cwriteln!(f, "<cyan>-</> <yellow>stdout:</>")?;
        cwriteln!(f, "<dim>{}</>", stdout.indent(2))?;
        Ok(())
    }
}

/// A query match didn't exist when it should have.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct QueryNotMatched {
    /// The query that didn't match.
    #[builder(into)]
    pub query: String,

    /// The path to the file that was checked.
    #[builder(into)]
    pub path: PathBuf,

    /// The language of the file that was checked.
    pub language: String,

    /// The content of the file that was checked.
    #[builder(into)]
    pub content: String,
}

impl Display for QueryNotMatched {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let query = &self.query;
        let path = self.path.display();
        let language = &self.language;
        let content = &self.content;
        cwriteln!(f, "<yellow>query did not match:</>")?;
        cwriteln!(f, "<cyan>-</> <yellow>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <yellow>language:</> {language}")?;
        cwriteln!(f, "<cyan>-</> <yellow>query:</>")?;
        cwriteln!(f, "<dim>{}</>", query.indent(2))?;
        cwriteln!(f, "<cyan>-</> <yellow>content:</>")?;
        cwriteln!(f, "<dim>{}</>", content.indent(2))?;
        Ok(())
    }
}

/// A query match existed when it shouldn't have.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct QueryMatched {
    /// The query that matched.
    #[builder(into)]
    pub query: String,

    /// The path to the file that was checked.
    #[builder(into)]
    pub path: PathBuf,

    /// The language of the file that was checked.
    pub language: Language,

    /// The content of the file that was checked.
    #[builder(into)]
    pub content: String,

    /// The matches that were found.
    #[builder(into)]
    pub matches: Matches,
}

impl Display for QueryMatched {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let query = &self.query;
        let path = self.path.display();
        let language = &self.language;
        let snippet = Snippet::new(&self.content);
        let content = snippet.render_highlighted(self.matches.spans());
        let tree = snippet
            .render_syntax_tree(self.language)
            .unwrap_or_else(|err| cformat!("<red>error rendering syntax tree:</> {err:?}"));

        cwriteln!(f, "<yellow>query matched:</>")?;
        cwriteln!(f, "<cyan>-</> <yellow>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <yellow>language:</> {language}")?;
        cwriteln!(f, "<cyan>-</> <yellow>query:</>")?;
        cwriteln!(f, "<dim>{}</>", query.indent(2))?;
        cwriteln!(f, "<cyan>-</> <yellow>syntax tree:</>")?;
        cwriteln!(f, "{}", tree.indent(2))?;
        cwriteln!(f, "<cyan>-</> <yellow>matched:</>")?;
        cwriteln!(f, "{}", content.indent(2))?;
        Ok(())
    }
}

/// A specific violation that caused an expectation to fail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Violation {
    /// An evaluation command failed.
    CommandFailed(CommandFailed),

    /// An evaluation command expected a query match to exist, but it didn't.
    QueryNotMatched(QueryNotMatched),

    /// An evaluation command expected a query match to not exist, but it did.
    QueryMatched(QueryMatched),
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandFailed(failed) => failed.fmt(f),
            Self::QueryNotMatched(not_matched) => not_matched.fmt(f),
            Self::QueryMatched(matched) => matched.fmt(f),
        }
    }
}
