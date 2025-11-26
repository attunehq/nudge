//! Evaluator for checking rule compliance in generated code.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{Result, WrapErr};
use regex::{Regex, RegexBuilder};

use crate::scenario::Scenario;

/// Result of evaluating a file for rule compliance.
#[derive(Debug, Clone)]
pub struct EvaluationResult {
    /// Path to the file that was checked.
    pub file_path: String,

    /// Whether the rule was followed (no violations found).
    pub passed: bool,

    /// Line numbers where violations were found.
    pub violation_lines: Vec<usize>,
}

/// Evaluate a scenario's output files for rule compliance.
pub fn evaluate_scenario(scenario: &Scenario, repo_dir: &Path) -> Result<Vec<EvaluationResult>> {
    let violation_pattern = RegexBuilder::new(&scenario.violation_pattern)
        .multi_line(scenario.multiline)
        .dot_matches_new_line(scenario.multiline)
        .build()
        .wrap_err_with(|| format!("Invalid violation pattern: {}", scenario.violation_pattern))?;

    let exclude_pattern = scenario
        .exclude_pattern
        .as_ref()
        .map(|p| Regex::new(p))
        .transpose()
        .wrap_err("Invalid exclude pattern")?;

    // Expand glob patterns and collect all files to check
    let files_to_check = expand_file_patterns(&scenario.check_files, repo_dir)?;

    // If no files found at all, that's a pass (no violations possible)
    if files_to_check.is_empty() {
        return Ok(vec![]);
    }

    let mut results = Vec::new();

    for full_path in files_to_check {
        let relative_path = full_path
            .strip_prefix(repo_dir)
            .unwrap_or(&full_path)
            .to_string_lossy()
            .to_string();

        let content = std::fs::read_to_string(&full_path)
            .wrap_err_with(|| format!("Failed to read file: {}", full_path.display()))?;

        let violation_lines = if scenario.multiline {
            find_multiline_violations(&content, &violation_pattern)
        } else {
            find_violations(&content, &violation_pattern, exclude_pattern.as_ref())
        };

        results.push(EvaluationResult {
            file_path: relative_path,
            passed: violation_lines.is_empty(),
            violation_lines,
        });
    }

    Ok(results)
}

/// Expand file patterns (including globs) to actual file paths.
fn expand_file_patterns(patterns: &[String], repo_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for pattern in patterns {
        let full_pattern = repo_dir.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        // Check if it's a glob pattern
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            for entry in glob::glob(&pattern_str).wrap_err("Invalid glob pattern")? {
                if let Ok(path) = entry {
                    if path.is_file() {
                        files.push(path);
                    }
                }
            }
        } else {
            // Direct file path - only add if it exists
            if full_pattern.exists() && full_pattern.is_file() {
                files.push(full_pattern);
            }
        }
    }

    Ok(files)
}

/// Find violations using multiline pattern matching.
fn find_multiline_violations(content: &str, pattern: &Regex) -> Vec<usize> {
    pattern
        .find_iter(content)
        .map(|m| content[..m.start()].matches('\n').count() + 1)
        .collect()
}

/// Find lines that match the violation pattern but not the exclude pattern.
fn find_violations(
    content: &str,
    violation_pattern: &Regex,
    exclude_pattern: Option<&Regex>,
) -> Vec<usize> {
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            if !violation_pattern.is_match(line) {
                return false;
            }
            if let Some(exclude) = exclude_pattern {
                if exclude.is_match(line) {
                    return false;
                }
            }
            true
        })
        .map(|(i, _)| i + 1)
        .collect()
}

/// Check if all evaluations passed.
pub fn all_passed(results: &[EvaluationResult]) -> bool {
    results.iter().all(|r| r.passed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_violations_simple() {
        let content = "fn main() {\n    use std::io;\n}";
        let pattern = Regex::new(r"^\s+use ").unwrap();
        let violations = find_violations(content, &pattern, None);
        assert_eq!(violations, vec![2]);
    }

    #[test]
    fn test_find_violations_with_exclude() {
        let content = "fn main() {\n    // use std::io;\n    use std::fs;\n}";
        let pattern = Regex::new(r"^\s+use ").unwrap();
        let exclude = Regex::new(r"^\s*//").unwrap();
        let violations = find_violations(content, &pattern, Some(&exclude));
        assert_eq!(violations, vec![3]);
    }

    #[test]
    fn test_find_violations_none() {
        let content = "use std::io;\n\nfn main() {}";
        let pattern = Regex::new(r"^\s+use ").unwrap();
        let violations = find_violations(content, &pattern, None);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_multiline_violations() {
        let content = "struct Foo {\n    a: String,\n    b: String,\n}";
        let pattern = Regex::new(r",\n[ \t]+\w+\s*:").unwrap();
        let violations = find_multiline_violations(content, &pattern);
        assert_eq!(violations, vec![2]);
    }

    #[test]
    fn test_multiline_violations_with_spacing() {
        let content = "struct Foo {\n    a: String,\n\n    b: String,\n}";
        let pattern = Regex::new(r",\n[ \t]+\w+\s*:").unwrap();
        let violations = find_multiline_violations(content, &pattern);
        assert!(violations.is_empty());
    }
}
