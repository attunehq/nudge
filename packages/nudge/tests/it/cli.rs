//! CLI Subcommand Smoke Tests

use crate::run_nudge;
use pretty_assertions::assert_eq as pretty_assert_eq;

#[test]
fn test_validate_discovers_config() {
    // Should find .nudge.yaml in packages/nudge/
    let (exit_code, stdout, _stderr) = run_nudge(&["validate"]);

    pretty_assert_eq!(exit_code, 0, "validate should exit 0");
    assert!(
        stdout.contains(".nudge.yaml") && stdout.contains("rules loaded"),
        "validate should report loaded rules, got: {stdout}"
    );
}

#[test]
fn test_validate_specific_file() {
    // Use CARGO_MANIFEST_DIR to get absolute path to the test config
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let config_path = format!("{manifest_dir}/.nudge.yaml");

    let (exit_code, stdout, _stderr) = run_nudge(&["validate", &config_path]);

    pretty_assert_eq!(exit_code, 0, "validate should exit 0");
    assert!(
        stdout.contains("rules loaded"),
        "validate should report loaded rules, got: {stdout}"
    );
    assert!(
        stdout.contains("no-inline-imports"),
        "validate should list rule names, got: {stdout}"
    );
}

#[test]
fn test_validate_nonexistent_file() {
    let (exit_code, stdout, _stderr) = run_nudge(&["validate", "nonexistent.yaml"]);

    pretty_assert_eq!(exit_code, 0, "validate should exit 0 for nonexistent file");
    assert!(
        stdout.contains("0 rules loaded"),
        "validate should report 0 rules for nonexistent file, got: {stdout}"
    );
}

#[test]
fn test_test_rule_match() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "test",
        "--rule",
        "no-inline-imports",
        "--tool",
        "Write",
        "--file",
        "test.rs",
        "--content",
        "fn main() {\n    use std::io;\n}",
    ]);

    pretty_assert_eq!(exit_code, 0, "test command should exit 0");
    assert!(
        stdout.contains("Rule: no-inline-imports"),
        "test should show rule name, got: {stdout}"
    );
    assert!(
        stdout.contains("INTERRUPT"),
        "test should show INTERRUPT for matching content, got: {stdout}"
    );
}

#[test]
fn test_test_rule_no_match() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "test",
        "--rule",
        "no-inline-imports",
        "--tool",
        "Write",
        "--file",
        "test.rs",
        "--content",
        "use std::io;\nfn main() {}",
    ]);

    pretty_assert_eq!(exit_code, 0, "test command should exit 0");
    assert!(
        stdout.contains("NO MATCH"),
        "test should show NO MATCH for non-matching content, got: {stdout}"
    );
}

#[test]
fn test_test_rule_not_found() {
    let (exit_code, _stdout, stderr) = run_nudge(&[
        "test",
        "--rule",
        "nonexistent-rule",
        "--content",
        "anything",
    ]);

    // Should fail with an error
    assert!(exit_code != 0, "test should fail for nonexistent rule");
    assert!(
        stderr.contains("not found") || stderr.contains("nonexistent-rule"),
        "test should report rule not found, got: {stderr}"
    );
}
