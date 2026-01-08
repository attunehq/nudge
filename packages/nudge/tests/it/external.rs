//! Integration tests for External matcher.
//!
//! These tests verify that external program matching works through the full
//! hook pipeline, including correct command execution and template
//! interpolation.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

/// Create a temporary directory with a .nudge.yaml config containing the given
/// rules.
fn setup_config(rules_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join(".nudge.yaml");
    std::fs::write(&config_path, rules_yaml).expect("write config");
    dir
}

/// Get the path to the built nudge binary.
fn get_binary_path() -> PathBuf {
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "nudge"])
        .status()
        .expect("failed to build nudge");
    assert!(status.success(), "cargo build failed");

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    workspace_root.join("target/debug/nudge")
}

/// Run nudge claude hook with the given input JSON in the specified directory.
fn run_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let binary = get_binary_path();

    let mut child = Command::new(&binary)
        .args(["claude", "hook"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn nudge");

    {
        let stdin = child.stdin.as_mut().expect("failed to get stdin");
        stdin
            .write_all(input.as_bytes())
            .expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait for nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let combined = if !stdout.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };

    (exit_code, combined)
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

#[test]
fn test_external_triggers_when_command_fails() {
    // grep -q exits 1 when pattern is NOT found, 0 when found
    // So we want to trigger when "FORBIDDEN" is NOT in the content
    // i.e., we want to block content that doesn't contain "REQUIRED"
    let config = r#"
version: 1
rules:
  - name: require-header
    description: Require REQUIRED marker in file
    message: "File must contain REQUIRED marker. Run `{{ $command }}` to verify."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: External
            command: ["grep", "-q", "REQUIRED"]
"#;

    let dir = setup_config(config);

    // Content WITHOUT "REQUIRED" - grep fails (exit 1), rule triggers
    let input = write_hook("test.txt", "This file has no marker");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt when REQUIRED is missing, got: {output}"
    );
    assert!(
        output.contains("REQUIRED marker"),
        "expected rule message in output, got: {output}"
    );
}

#[test]
fn test_external_passes_when_command_succeeds() {
    let config = r#"
version: 1
rules:
  - name: require-header
    description: Require REQUIRED marker in file
    message: "File must contain REQUIRED marker."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: External
            command: ["grep", "-q", "REQUIRED"]
"#;

    let dir = setup_config(config);

    // Content WITH "REQUIRED" - grep succeeds (exit 0), rule passes
    let input = write_hook("test.txt", "This file has REQUIRED marker");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough when REQUIRED is present, got: {output}"
    );
}

#[test]
fn test_external_command_capture_interpolation() {
    let config = r#"
version: 1
rules:
  - name: no-fixme
    description: Block FIXME comments
    message: "Remove FIXME comments. Debug with: {{ $command }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: External
            command: ["grep", "-qv", "FIXME"]
"#;

    let dir = setup_config(config);

    // grep -qv exits 1 if pattern IS found (inverted match)
    let input = write_hook("test.rs", "// FIXME: fix this later");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for FIXME comment, got: {output}"
    );
    // Check that $command was interpolated
    assert!(
        output.contains("grep -qv FIXME"),
        "expected command to be interpolated in message, got: {output}"
    );
}

#[test]
fn test_external_file_pattern_filtering() {
    let config = r#"
version: 1
rules:
  - name: require-header
    description: Only applies to .txt files
    message: "Missing header"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: External
            command: ["grep", "-q", "HEADER"]
"#;

    let dir = setup_config(config);

    // .rs file should not be checked (file pattern doesn't match)
    let input = write_hook("test.rs", "No header here");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for non-.txt file, got: {output}"
    );
}
