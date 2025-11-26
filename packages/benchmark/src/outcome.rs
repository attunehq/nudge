//! Structured outcomes for benchmark evaluations.
//!
//! This module provides types for representing the results of benchmark runs
//! in a structured way that supports:
//! - Nice CLI rendering
//! - JSON serialization for reports
//! - Aggregation across multiple runs

use std::path::PathBuf;

use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};

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
            Self::Pass => write!(f, "{} Passed", "✓".green()),
            Self::Fail { violations } => {
                writeln!(f, "{} Failed", "✗".red())?;
                for violation in violations {
                    write!(f, "{violation}")?;
                }
                Ok(())
            }
        }
    }
}

/// A specific violation that caused an expectation to fail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Violation {
    /// A regex pattern matched when it shouldn't have (not_contains failed).
    RegexMatched {
        /// The file where the match was found.
        path: PathBuf,

        /// The regex pattern that matched.
        pattern: String,

        /// The text that matched the pattern.
        matched: String,

        /// Line number where the match starts (1-indexed).
        line: usize,
    },

    /// A regex pattern didn't match when it should have (contains failed).
    RegexNotMatched {
        /// The file that was checked.
        path: PathBuf,

        /// The regex pattern that didn't match.
        pattern: String,
    },

    /// A string was found when it shouldn't have been (not_contains failed).
    StringFound {
        /// The file where the string was found.
        path: PathBuf,

        /// The string that was found.
        needle: String,

        /// Line number where the string was found (1-indexed).
        line: usize,
    },

    /// A string wasn't found when it should have been (contains failed).
    StringNotFound {
        /// The file that was checked.
        path: PathBuf,

        /// The string that wasn't found.
        needle: String,
    },

    /// File content didn't match expected content exactly (equals failed).
    ContentMismatch {
        /// The file that was checked.
        path: PathBuf,

        /// Description of what was expected.
        expected: String,
    },

    /// A command failed during evaluation.
    CommandFailed {
        /// The command that failed.
        command: String,

        /// The exit code, if available.
        exit_code: Option<i32>,

        /// Stderr output, if any.
        stderr: Option<String>,
    },
}

impl Violation {
    /// Create a RegexMatched violation, computing the line number from content.
    pub fn regex_matched(path: PathBuf, pattern: &str, content: &str, matched: &str) -> Self {
        let line = Self::find_line_number(content, matched);
        Self::RegexMatched {
            path,
            pattern: pattern.to_string(),
            matched: matched.to_string(),
            line,
        }
    }

    /// Create a StringFound violation, computing the line number from content.
    pub fn string_found(path: PathBuf, needle: &str, content: &str) -> Self {
        let line = content
            .find(needle)
            .map(|pos| Self::find_line_number(content, &content[pos..pos + needle.len()]))
            .unwrap_or(1);
        Self::StringFound {
            path,
            needle: needle.to_string(),
            line,
        }
    }

    fn find_line_number(content: &str, matched: &str) -> usize {
        let match_start = matched.as_ptr() as usize - content.as_ptr() as usize;
        content[..match_start].lines().count() + 1
    }
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegexMatched {
                path,
                pattern,
                matched,
                line,
            } => {
                writeln!(
                    f,
                    "    {} {} {}",
                    "regex matched:".yellow(),
                    path.display().to_string().dimmed(),
                    format!("line {line}").dimmed()
                )?;
                writeln!(f, "      {} {}", "pattern:".green().bold(), pattern.dimmed())?;
                writeln!(f, "      {} {:?}", "matched:".green().bold(), matched.dimmed())
            }
            Self::RegexNotMatched { path, pattern } => {
                writeln!(
                    f,
                    "    {} {}",
                    "regex not matched:".yellow(),
                    path.display().to_string().dimmed()
                )?;
                writeln!(f, "      {} {}", "pattern:".green().bold(), pattern.dimmed())
            }
            Self::StringFound { path, needle, line } => {
                writeln!(
                    f,
                    "    {} {} {}",
                    "string found:".yellow(),
                    path.display().to_string().dimmed(),
                    format!("line {line}").dimmed()
                )?;
                writeln!(f, "      {} {:?}", "string:".green().bold(), needle.dimmed())
            }
            Self::StringNotFound { path, needle } => {
                writeln!(
                    f,
                    "    {} {}",
                    "string not found:".yellow(),
                    path.display().to_string().dimmed()
                )?;
                writeln!(f, "      {} {:?}", "expected:".green().bold(), needle.dimmed())
            }
            Self::ContentMismatch { path, expected } => {
                writeln!(
                    f,
                    "    {} {}",
                    "content mismatch:".yellow(),
                    path.display().to_string().dimmed()
                )?;
                writeln!(f, "      {} {}", "expected:".green().bold(), expected.dimmed())
            }
            Self::CommandFailed {
                command,
                exit_code,
                stderr,
            } => {
                writeln!(f, "    {} {}", "command failed:".yellow(), command.dimmed())?;
                if let Some(code) = exit_code {
                    writeln!(f, "      {} {}", "exit code:".green().bold(), code)?;
                }
                if let Some(err) = stderr {
                    if !err.is_empty() {
                        writeln!(f, "      {} {}", "stderr:".green().bold(), err.dimmed())?;
                    }
                }
                Ok(())
            }
        }
    }
}
