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
    Pass {
        /// Evidence of why the expectations passed.
        evidence: Vec<Evidence>,
    },

    /// One or more expectations failed.
    Fail {
        /// The violations that caused the failure.
        violations: Vec<Violation>,
    },
}

impl Outcome {
    /// Create a passing outcome with no evidence.
    pub fn pass() -> Self {
        Self::Pass { evidence: vec![] }
    }

    /// Create a passing outcome with evidence of why it passed.
    pub fn pass_with_evidence(evidence: Vec<Evidence>) -> Self {
        Self::Pass { evidence }
    }

    /// Create a failing outcome with violations.
    pub fn fail(violations: Vec<Violation>) -> Self {
        Self::Fail { violations }
    }

    /// Returns true if this outcome is a pass.
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass { .. })
    }

    /// Returns true if this outcome is a failure.
    pub fn is_fail(&self) -> bool {
        matches!(self, Self::Fail { .. })
    }

    /// Returns the violations if this is a failure, or an empty slice if it's a
    /// pass.
    pub fn violations(&self) -> &[Violation] {
        match self {
            Self::Pass { .. } => &[],
            Self::Fail { violations } => violations,
        }
    }

    /// Returns the evidence if this is a pass, or an empty slice if it's a
    /// failure.
    pub fn evidence(&self) -> &[Evidence] {
        match self {
            Self::Pass { evidence } => evidence,
            Self::Fail { .. } => &[],
        }
    }

    /// Combine multiple outcomes into one.
    ///
    /// If any outcome is a failure, the combined result is a failure with all
    /// violations. Otherwise, the combined result is a pass with all
    /// evidence.
    pub fn combine(outcomes: impl IntoIterator<Item = Outcome>) -> Outcome {
        let mut all_violations = Vec::new();
        let mut all_evidence = Vec::new();

        for outcome in outcomes {
            match outcome {
                Outcome::Pass { evidence } => all_evidence.extend(evidence),
                Outcome::Fail { violations } => all_violations.extend(violations),
            }
        }

        if all_violations.is_empty() {
            Outcome::Pass {
                evidence: all_evidence,
            }
        } else {
            Outcome::Fail {
                violations: all_violations,
            }
        }
    }
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass { evidence } => {
                writeln!(f, "{}", cformat!("<green>✓</> Passed"))?;
                for ev in evidence {
                    write!(f, "{}", ev.to_string().indent(2))?;
                }
                Ok(())
            }
            Self::Fail { violations } => {
                writeln!(f, "{}", cformat!("<red>✗</> Failed"))?;
                for violation in violations {
                    write!(f, "{}", violation.to_string().indent(2))?;
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
pub struct QueryNotMatchedViolation {
    /// The query that didn't match.
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

    /// Description of any filter applied to the query results.
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl Display for QueryNotMatchedViolation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let query = &self.query;
        let path = self.path.display();
        let language = self.language;
        let content = &self.content;
        let filter = self.filter.as_deref().unwrap_or("<no filter applied>");

        cwriteln!(f, "<yellow>query did not match:</>")?;
        cwriteln!(f, "<cyan>-</> <yellow>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <yellow>language:</> {language}")?;
        cwriteln!(f, "<cyan>-</> <yellow>query:</>")?;
        cwriteln!(f, "<dim>{}</>", query.indent(2))?;
        cwriteln!(f, "<cyan>-</> <yellow>filter:</>")?;
        cwriteln!(f, "{}", filter.indent(2))?;
        cwriteln!(f, "<cyan>-</> <yellow>content:</>")?;
        cwriteln!(f, "<dim>{}</>", content.indent(2))?;
        Ok(())
    }
}

/// A query match existed when it shouldn't have.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct QueryMatchedViolation {
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

    /// Description of any filter applied to the query results.
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl Display for QueryMatchedViolation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let query = &self.query;
        let path = self.path.display();
        let language = &self.language;
        let filter = self.filter.as_deref().unwrap_or("<no filter applied>");
        let snippet = Snippet::new(&self.content);
        let content = snippet.render_highlighted(self.matches.spans());

        cwriteln!(f, "<yellow>query matched:</>")?;
        cwriteln!(f, "<cyan>-</> <yellow>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <yellow>language:</> {language}")?;
        cwriteln!(f, "<cyan>-</> <yellow>query:</>")?;
        cwriteln!(f, "<dim>{}</>", query.indent(2))?;
        cwriteln!(f, "<cyan>-</> <yellow>filter:</>")?;
        cwriteln!(f, "{}", filter.indent(2))?;
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
    QueryNotMatched(QueryNotMatchedViolation),

    /// An evaluation command expected a query match to not exist, but it did.
    QueryMatched(QueryMatchedViolation),
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

/// Evidence that an expectation was satisfied.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Evidence {
    /// A command succeeded.
    CommandSucceeded(CommandSucceeded),

    /// A query matched as expected.
    QueryMatched(QueryMatchedEvidence),

    /// A query did not match as expected.
    QueryNotMatched(QueryNotMatchedEvidence),
}

impl std::fmt::Display for Evidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandSucceeded(succeeded) => succeeded.fmt(f),
            Self::QueryMatched(matched) => matched.fmt(f),
            Self::QueryNotMatched(not_matched) => not_matched.fmt(f),
        }
    }
}

/// A command succeeded during evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct CommandSucceeded {
    /// The command that succeeded.
    #[builder(into)]
    pub command: String,

    /// The exit code.
    pub exit_code: Option<i32>,

    /// Stdout output, if any.
    #[builder(into)]
    pub stdout: Option<String>,

    /// Stderr output, if any.
    #[builder(into)]
    pub stderr: Option<String>,
}

impl Display for CommandSucceeded {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let command = &self.command;
        let exit_code = self.exit_code.unwrap_or_default();
        cwriteln!(f, "<green>command succeeded:</> <dim>{command}</>")?;
        cwriteln!(f, "<cyan>-</> <green>exit code:</> {exit_code}")?;
        if let Some(stdout) = &self.stdout
            && !stdout.is_empty()
        {
            cwriteln!(f, "<cyan>-</> <green>stdout:</>")?;
            cwriteln!(f, "<dim>{}</>", stdout.indent(2))?;
        }
        Ok(())
    }
}

/// A query matched as expected (evidence of success).
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct QueryMatchedEvidence {
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

    /// Description of any filter applied to the query results.
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl Display for QueryMatchedEvidence {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let query = &self.query;
        let path = self.path.display();
        let language = &self.language;
        let snippet = Snippet::new(&self.content);
        let content = snippet.render_highlighted_green(self.matches.spans());
        let filter = self.filter.as_deref().unwrap_or("<no filter applied>");

        cwriteln!(f, "<green>query matched:</>")?;
        cwriteln!(f, "<cyan>-</> <green>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <green>language:</> {language}")?;
        cwriteln!(f, "<cyan>-</> <green>query:</>")?;
        cwriteln!(f, "<dim>{}</>", query.indent(2))?;
        cwriteln!(f, "<cyan>-</> <green>filter:</>")?;
        cwriteln!(f, "{}", filter.indent(2))?;
        cwriteln!(f, "<cyan>-</> <green>matched:</>")?;
        cwriteln!(f, "{}", content.indent(2))?;
        Ok(())
    }
}

/// A query did not match as expected (evidence of success).
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct QueryNotMatchedEvidence {
    /// The query that didn't match.
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

    /// Description of any filter applied to the query results.
    #[builder(into)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl Display for QueryNotMatchedEvidence {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let query = &self.query;
        let path = self.path.display();
        let language = self.language;
        let snippet = Snippet::new(&self.content);
        let content = snippet.render();
        let filter = self.filter.as_deref().unwrap_or("<no filter applied>");

        cwriteln!(f, "<green>query did not match:</>")?;
        cwriteln!(f, "<cyan>-</> <green>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <green>language:</> {language}")?;
        cwriteln!(f, "<cyan>-</> <green>query:</>")?;
        cwriteln!(f, "<dim>{}</>", query.indent(2))?;
        cwriteln!(f, "<cyan>-</> <green>filter:</>")?;
        cwriteln!(f, "{}", filter.indent(2))?;
        cwriteln!(f, "<cyan>-</> <green>content:</>")?;
        cwriteln!(f, "<dim>{}</>", content.indent(2))?;
        Ok(())
    }
}
