//! Integration tests for the user-defined rules system.
//!
//! These tests verify that the YAML-based rule system works correctly:
//! - Rules are loaded from .pavlov.yaml
//! - Rules match based on hook type, tool name, file pattern, and content
//! - Rules produce correct responses (interrupt vs continue vs passthrough)

use pretty_assertions::assert_eq as pretty_assert_eq;
use simple_test_case::test_case;
use xshell::Shell;

/// Expected outcome from running a hook through pavlov.
#[derive(Debug, Clone, PartialEq)]
enum Expected {
    /// Passthrough: exit 0, no output
    Passthrough,

    /// Continue: exit 0, output contains "continue":true
    #[allow(dead_code)]
    Continue,

    /// Interrupt: exit 2, output contains "continue":false
    Interrupt,
}

/// Build a PreToolUse hook JSON payload for Write tool.
fn write_hook(file_path: &str, content: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "tool_name": "Write",
        "tool_use_id": "123",
        "tool_input": {
            "file_path": file_path,
            "content": content
        }
    })
    .to_string()
}

/// Build a PreToolUse hook JSON payload for Edit tool.
fn edit_hook(file_path: &str, old_string: &str, new_string: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "tool_name": "Edit",
        "tool_use_id": "123",
        "tool_input": {
            "file_path": file_path,
            "old_string": old_string,
            "new_string": new_string
        }
    })
    .to_string()
}

/// Build a UserPromptSubmit hook JSON payload.
fn user_prompt_hook(prompt: &str) -> String {
    serde_json::json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "prompt": prompt
    })
    .to_string()
}

/// Run pavlov claude hook with the given input JSON and return (exit_code, output).
fn run_hook(_sh: &Shell, input: &str) -> (i32, String) {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Build and get the binary path
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "pavlov"])
        .status()
        .expect("failed to build pavlov");
    assert!(status.success(), "cargo build failed");

    // Run the binary directly with stdin
    let mut child = Command::new("cargo")
        .args(["run", "--quiet", "-p", "pavlov", "--", "claude", "hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn pavlov");

    // Write input to stdin
    {
        let stdin = child.stdin.as_mut().expect("failed to get stdin");
        stdin.write_all(input.as_bytes()).expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait for pavlov");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);


    // For interrupt responses, output goes to stderr
    // For continue responses, output goes to stdout
    // Combine them but filter out compiler warnings
    let is_json = |line: &str| {
        let trimmed = line.trim();
        trimmed.starts_with('{') || trimmed.starts_with('"')
    };

    let combined = if !stdout.trim().is_empty() && is_json(stdout.trim()) {
        stdout.trim().to_string()
    } else if !stderr.trim().is_empty() {
        // Filter out compiler warnings from stderr
        stderr
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with("warning:")
                    && !trimmed.contains("--> packages/")
                    && !trimmed.contains("= note:")
                    && !trimmed.starts_with('|')
                    && !trimmed.is_empty()
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };

    (exit_code, combined)
}

fn assert_expected(exit_code: i32, output: &str, expected: Expected) {
    match expected {
        Expected::Passthrough => {
            pretty_assert_eq!(exit_code, 0, "expected passthrough (exit 0)");
            assert!(output.is_empty(), "expected no output for passthrough, got: {output}");
        }
        Expected::Continue => {
            pretty_assert_eq!(exit_code, 0, "expected continue (exit 0)");
            assert!(
                output.contains(r#""continue":true"#),
                "expected continue:true in output, got: {output}"
            );
        }
        Expected::Interrupt => {
            pretty_assert_eq!(
                exit_code, 2,
                "expected interrupt (exit 2), output: {output}"
            );
            assert!(
                output.contains(r#""continue":false"#),
                "expected continue:false in output, got: {output}"
            );
        }
    }
}

// =============================================================================
// Basic Rule Loading Tests
// =============================================================================

#[test]
fn test_no_rules_passthrough() {
    // Non-matching file extension should passthrough (no rules match)
    let sh = Shell::new().unwrap();
    let input = write_hook("test.xyz", "any content");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}

// =============================================================================
// Inline Imports Rule Tests
// =============================================================================

#[test_case(
    "fn main() {\n    use std::io;\n}",
    Expected::Interrupt;
    "indented use statement triggers interrupt"
)]
#[test_case(
    "use std::io;\n\nfn main() {}",
    Expected::Passthrough;
    "top-level use statement passes"
)]
#[test]
fn test_inline_imports(content: &str, expected: Expected) {
    let sh = Shell::new().unwrap();
    let input = write_hook("test.rs", content);
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, expected);
}

// =============================================================================
// Edit Tool Tests
// =============================================================================

#[test]
fn test_edit_tool_content_matching() {
    let sh = Shell::new().unwrap();
    // Edit that introduces an indented use statement
    let input = edit_hook("test.rs", "old code", "    use std::io;\n");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn test_edit_tool_non_matching() {
    let sh = Shell::new().unwrap();
    // Edit that doesn't trigger any rules
    let input = edit_hook("test.rs", "old", "new");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}

// =============================================================================
// Non-Rust File Tests
// =============================================================================

#[test_case("test.py", "    use std::io;"; "python file passes")]
#[test_case("test.js", "    use std::io;"; "javascript file passes")]
#[test_case("test.txt", "let foo: Type = bar;"; "text file passes")]
#[test]
fn test_non_rust_files_pass(file_path: &str, content: &str) {
    let sh = Shell::new().unwrap();
    let input = write_hook(file_path, content);
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}

// =============================================================================
// UserPromptSubmit Hook Tests
// =============================================================================

#[test]
fn test_user_prompt_no_matching_rules() {
    let sh = Shell::new().unwrap();
    let input = user_prompt_hook("hello world");
    let (exit_code, output) = run_hook(&sh, &input);
    // No UserPromptSubmit rules in the test config, so should passthrough
    assert_expected(exit_code, &output, Expected::Passthrough);
}

// =============================================================================
// Message Content Tests
// =============================================================================

#[test]
fn test_interrupt_message_contains_rule_message() {
    let sh = Shell::new().unwrap();
    let input = write_hook("test.rs", "fn main() {\n    use std::io;\n}");
    let (_, output) = run_hook(&sh, &input);

    // Should contain the message from the no-inline-imports rule
    assert!(
        output.contains("Move the `use` statement"),
        "expected rule message in output"
    );
}

// =============================================================================
// Multiple Rules Tests
// =============================================================================

#[test]
fn test_multiple_rules_fire() {
    let sh = Shell::new().unwrap();
    // Content that triggers both inline imports AND lhs type annotations
    let content = "fn main() {\n    use std::io;\n    let foo: Vec<String> = vec![];\n}";
    let input = write_hook("test.rs", content);
    let (exit_code, output) = run_hook(&sh, &input);

    // Should be interrupt (any interrupt = overall interrupt)
    pretty_assert_eq!(exit_code, 2, "expected interrupt when multiple rules fire");

    // Messages should be concatenated with separator
    assert!(
        output.contains("---") || output.contains("Move the `use` statement"),
        "expected messages from multiple rules"
    );
}

// =============================================================================
// CLI Subcommand Smoke Tests
// =============================================================================

/// Run a pavlov subcommand and return (exit_code, stdout, stderr).
fn run_pavlov(args: &[&str]) -> (i32, String, String) {
    use std::process::{Command, Stdio};

    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "pavlov"])
        .status()
        .expect("failed to build pavlov");
    assert!(status.success(), "cargo build failed");

    let mut cmd_args = vec!["run", "--quiet", "-p", "pavlov", "--"];
    cmd_args.extend(args);

    let output = Command::new("cargo")
        .args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run pavlov");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}

#[test]
fn test_validate_discovers_config() {
    // Should find .pavlov.yaml in packages/pavlov/
    let (exit_code, stdout, _stderr) = run_pavlov(&["validate"]);

    pretty_assert_eq!(exit_code, 0, "validate should exit 0");
    assert!(
        stdout.contains(".pavlov.yaml") && stdout.contains("rules loaded"),
        "validate should report loaded rules, got: {stdout}"
    );
}

#[test]
fn test_validate_specific_file() {
    // Use CARGO_MANIFEST_DIR to get absolute path to the test config
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let config_path = format!("{manifest_dir}/.pavlov.yaml");

    let (exit_code, stdout, _stderr) =
        run_pavlov(&["validate", &config_path]);

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
    let (exit_code, stdout, _stderr) =
        run_pavlov(&["validate", "nonexistent.yaml"]);

    pretty_assert_eq!(exit_code, 0, "validate should exit 0 for nonexistent file");
    assert!(
        stdout.contains("0 rules loaded"),
        "validate should report 0 rules for nonexistent file, got: {stdout}"
    );
}

#[test]
fn test_test_rule_match() {
    let (exit_code, stdout, _stderr) = run_pavlov(&[
        "test",
        "--rule", "no-inline-imports",
        "--tool", "Write",
        "--file", "test.rs",
        "--content", "fn main() {\n    use std::io;\n}",
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
    let (exit_code, stdout, _stderr) = run_pavlov(&[
        "test",
        "--rule", "no-inline-imports",
        "--tool", "Write",
        "--file", "test.rs",
        "--content", "use std::io;\nfn main() {}",
    ]);

    pretty_assert_eq!(exit_code, 0, "test command should exit 0");
    assert!(
        stdout.contains("NO MATCH"),
        "test should show NO MATCH for non-matching content, got: {stdout}"
    );
}

#[test]
fn test_test_rule_not_found() {
    let (exit_code, _stdout, stderr) = run_pavlov(&[
        "test",
        "--rule", "nonexistent-rule",
        "--content", "anything",
    ]);

    // Should fail with an error
    assert!(exit_code != 0, "test should fail for nonexistent rule");
    assert!(
        stderr.contains("not found") || stderr.contains("nonexistent-rule"),
        "test should report rule not found, got: {stderr}"
    );
}
