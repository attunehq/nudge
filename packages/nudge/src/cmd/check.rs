//! Check project files against configured rules.
//!
//! This command validates all files in the project against Nudge rules,
//! enabling use in CI pipelines or as a standalone linter.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use clap::Args;
use color_eyre::eyre::{Context, Result};
use glob::Pattern;
use ignore::WalkBuilder;

use nudge::rules::{self, GlobMatcher, Hook, PreToolUseMatcher, Rule};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Paths or glob patterns to check. If not specified, checks entire project.
    #[arg()]
    pub paths: Vec<PathBuf>,
}

/// An issue found during checking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Issue {
    /// Path to the file containing the issue.
    file: PathBuf,
    /// Line number (1-indexed) where the issue starts.
    line: usize,
    /// Name of the rule that was violated.
    rule_name: String,
    /// Message describing the issue.
    message: String,
}

/// A file pattern extracted from a rule, with the matchers to apply.
struct FileRule<'a> {
    /// The glob pattern for matching files.
    pattern: &'a GlobMatcher,
    /// The content matchers to apply.
    matchers: ContentMatcherSet<'a>,
    /// Reference to the parent rule.
    rule: &'a Rule,
}

/// Content matchers extracted from a rule hook.
enum ContentMatcherSet<'a> {
    Write(&'a [nudge::rules::ContentMatcher]),
    Edit(&'a [nudge::rules::ContentMatcher]),
}

pub fn main(config: Config) -> Result<()> {
    let rules_by_source = rules::load_all_attributed().context("load rules")?;

    // Collect file rules from all sources
    let mut file_rules = Vec::new();
    let mut total_rules = 0;

    for (_source, rules) in &rules_by_source {
        for rule in rules {
            total_rules += 1;
            // Extract Write hooks
            for hook in &rule.on {
                match hook {
                    Hook::PreToolUse(PreToolUseMatcher::Write(matcher)) => {
                        file_rules.push(FileRule {
                            pattern: &matcher.file,
                            matchers: ContentMatcherSet::Write(&matcher.content),
                            rule,
                        });
                    }
                    Hook::PreToolUse(PreToolUseMatcher::Edit(matcher)) => {
                        file_rules.push(FileRule {
                            pattern: &matcher.file,
                            matchers: ContentMatcherSet::Edit(&matcher.new_content),
                            rule,
                        });
                    }
                    // Skip WebFetch, Bash, and UserPromptSubmit - they don't apply to file content
                    _ => {}
                }
            }
        }
    }

    if file_rules.is_empty() {
        println!("No file-based rules found.");
        return Ok(());
    }

    // Walk the project and collect files to check
    let files = collect_files(&config.paths)?;

    // Check each file against matching rules
    // Use a HashSet to deduplicate issues (same rule can have Write and Edit hooks
    // with identical matchers)
    let mut issues_set = HashSet::new();
    let mut checked_files = 0;

    for file in &files {
        let mut file_checked = false;

        for file_rule in &file_rules {
            if !file_rule.pattern.is_match_path(file) {
                continue;
            }

            // Read file content
            let content = match fs::read_to_string(file) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!(?file, error = %e, "skipping file (could not read)");
                    continue;
                }
            };

            file_checked = true;

            // Get the matchers based on hook type
            let matchers = match &file_rule.matchers {
                ContentMatcherSet::Write(m) => *m,
                ContentMatcherSet::Edit(m) => *m,
            };

            // Check content against matchers
            // A rule matches if ALL matchers match (AND logic), so we need to check
            // if all matchers have at least one match
            let all_matched = matchers.iter().all(|m| m.is_match(&content));

            if all_matched && !matchers.is_empty() {
                // Collect matches from all matchers for reporting
                for matcher in matchers {
                    let matches = matcher.matches_with_context(&content);
                    for m in matches {
                        let line = byte_offset_to_line(&content, m.span.start);
                        let message =
                            nudge::template::interpolate(&file_rule.rule.message, &m.captures);
                        issues_set.insert(Issue {
                            file: file.clone(),
                            line,
                            rule_name: file_rule.rule.name.clone(),
                            message,
                        });
                    }
                }
            }
        }

        if file_checked {
            checked_files += 1;
        }
    }

    // Convert to sorted Vec for deterministic output
    let mut issues = issues_set.into_iter().collect::<Vec<_>>();
    issues.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.rule_name.cmp(&b.rule_name))
    });

    // Output results
    if issues.is_empty() {
        print_success(checked_files, total_rules, &rules_by_source);
        Ok(())
    } else {
        print_failure(&issues, checked_files, total_rules);
        process::exit(1);
    }
}

/// Collect files to check based on provided paths or entire project.
fn collect_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if paths.is_empty() {
        // Walk entire project from current directory
        for entry in WalkBuilder::new(".").hidden(false).build() {
            let entry = entry.context("walk directory")?;
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                files.push(entry.into_path());
            }
        }
    } else {
        // Process each provided path
        for path in paths {
            let path_str = path.to_string_lossy();

            // Check if it's a glob pattern
            if path_str.contains('*') || path_str.contains('?') || path_str.contains('[') {
                let pattern = Pattern::new(&path_str)
                    .with_context(|| format!("invalid glob pattern: {path_str}"))?;

                // Walk from current directory and filter by pattern
                for entry in WalkBuilder::new(".").hidden(false).build() {
                    let entry = entry.context("walk directory")?;
                    if entry.file_type().is_some_and(|ft| ft.is_file())
                        && pattern.matches_path(entry.path())
                    {
                        files.push(entry.into_path());
                    }
                }
            } else if path.is_dir() {
                // Walk the directory
                for entry in WalkBuilder::new(path).hidden(false).build() {
                    let entry = entry.context("walk directory")?;
                    if entry.file_type().is_some_and(|ft| ft.is_file()) {
                        files.push(entry.into_path());
                    }
                }
            } else if path.is_file() {
                files.push(path.clone());
            } else {
                tracing::warn!(?path, "path does not exist, skipping");
            }
        }
    }

    Ok(files)
}

/// Convert a byte offset to a 1-indexed line number.
fn byte_offset_to_line(content: &str, offset: usize) -> usize {
    content[..offset.min(content.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count()
        + 1
}

/// Print success message.
fn print_success(
    checked_files: usize,
    total_rules: usize,
    rules_by_source: &[(PathBuf, Vec<Rule>)],
) {
    println!(
        "\u{2713} Checked {} files against {} rules",
        checked_files, total_rules
    );
    for (source, rules) in rules_by_source {
        if !rules.is_empty() {
            println!(
                "  - {}: {} {}",
                source.display(),
                rules.len(),
                if rules.len() == 1 { "rule" } else { "rules" }
            );
        }
    }
}

/// Print failure message with issues.
fn print_failure(issues: &[Issue], checked_files: usize, total_rules: usize) {
    // Group issues by file for cleaner output
    let mut issues_by_file = HashMap::<&Path, Vec<&Issue>>::new();
    for issue in issues {
        issues_by_file.entry(&issue.file).or_default().push(issue);
    }

    let file_count = issues_by_file.len();
    println!(
        "\u{2717} Found {} {} in {} {}",
        issues.len(),
        if issues.len() == 1 { "issue" } else { "issues" },
        file_count,
        if file_count == 1 { "file" } else { "files" }
    );
    println!();

    // Sort files for deterministic output
    let mut files = issues_by_file.keys().collect::<Vec<_>>();
    files.sort();

    for file in files {
        let file_issues = &issues_by_file[file];
        for issue in file_issues {
            println!(
                "{}:{} [{}]",
                issue.file.display(),
                issue.line,
                issue.rule_name
            );
            println!("  {}", issue.message);
            println!();
        }
    }

    println!(
        "Checked {} files against {} rules",
        checked_files, total_rules
    );
}
