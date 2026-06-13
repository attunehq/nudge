//! Integration tests for External matcher.
//!
//! These tests verify that external program matching works through the full
//! hook pipeline, including correct command execution and template
//! interpolation.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::nudge_binary;
use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

/// Create a temporary directory with a .nudge.yaml config containing the given
/// rules.
fn setup_config(rules_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join(".nudge.yaml");
    fs::write(&config_path, rules_yaml).expect("write config");
    dir
}

fn yaml_command(command: &[String]) -> String {
    serde_json::to_string(command).expect("serialize command")
}

fn required_marker_command(marker: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![String::from("findstr"), marker.to_string()]
    }

    #[cfg(not(windows))]
    {
        vec![String::from("grep"), String::from("-q"), marker.to_string()]
    }
}

fn no_fixme_command() -> Vec<String> {
    #[cfg(windows)]
    {
        vec![
            String::from("findstr"),
            String::from("/V"),
            String::from("FIXME"),
        ]
    }

    #[cfg(not(windows))]
    {
        vec![
            String::from("grep"),
            String::from("-qv"),
            String::from("FIXME"),
        ]
    }
}

fn slow_command() -> Vec<String> {
    #[cfg(windows)]
    {
        vec![
            String::from("powershell.exe"),
            String::from("-NoProfile"),
            String::from("-Command"),
            String::from("Start-Sleep -Seconds 20"),
        ]
    }

    #[cfg(not(windows))]
    {
        vec![
            String::from("sh"),
            String::from("-c"),
            String::from("sleep 20"),
        ]
    }
}

/// Run nudge claude hook with the given input JSON in the specified directory.
fn run_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let mut child = Command::new(nudge_binary())
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
    let command = yaml_command(&required_marker_command("REQUIRED"));
    // grep -q exits 1 when pattern is NOT found, 0 when found
    // So we want to trigger when "FORBIDDEN" is NOT in the content
    // i.e., we want to block content that doesn't contain "REQUIRED"
    let config = format!(
        r#"
version: 1
rules:
  - name: require-header
    description: Require REQUIRED marker in file
    message: "File must contain REQUIRED marker. Run `{{{{ $command }}}}` to verify."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: External
            command: {command}
"#
    );

    let dir = setup_config(&config);

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
    let command = yaml_command(&required_marker_command("REQUIRED"));
    let config = format!(
        r#"
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
            command: {command}
"#
    );

    let dir = setup_config(&config);

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
    let command = no_fixme_command();
    let expected_command = shell_words::join(&command);
    let command = yaml_command(&command);
    let config = format!(
        r#"
version: 1
rules:
  - name: no-fixme
    description: Block FIXME comments
    message: "Remove FIXME comments. Debug with: {{{{ $command }}}}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: External
            command: {command}
"#
    );

    let dir = setup_config(&config);

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
        output.contains(&expected_command),
        "expected command to be interpolated in message, got: {output}"
    );
}

#[test]
fn test_external_file_pattern_filtering() {
    let command = yaml_command(&required_marker_command("HEADER"));
    let config = format!(
        r#"
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
            command: {command}
"#
    );

    let dir = setup_config(&config);

    // .rs file should not be checked (file pattern doesn't match)
    let input = write_hook("test.rs", "No header here");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for non-.txt file, got: {output}"
    );
}

#[test]
fn test_external_missing_command_interrupts() {
    let config = r#"
version: 1
rules:
  - name: external-command-required
    description: External command must be available
    message: "External check failed: {{ $external_status }}. Command: {{ $command }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: External
            command: ["definitely-not-a-real-nudge-command"]
"#;

    let dir = setup_config(config);

    let input = write_hook("test.txt", "Any content");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt when external command cannot spawn, got: {output}"
    );
    assert!(
        output.contains("failed to start"),
        "expected spawn failure status in output, got: {output}"
    );
    assert!(
        output.contains("definitely-not-a-real-nudge-command"),
        "expected command capture in output, got: {output}"
    );
}

#[test]
fn test_external_timeout_interrupts_promptly() {
    let command = yaml_command(&slow_command());
    let config = format!(
        r#"
version: 1
rules:
  - name: external-command-timeout
    description: External command must finish promptly
    message: "External check failed: {{{{ $external_status }}}}. Command: {{{{ $command }}}}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: External
            command: {command}
            timeout_ms: 100
"#
    );

    let dir = setup_config(&config);

    let input = write_hook("test.txt", "Any content");
    let started = Instant::now();
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    let elapsed = started.elapsed();

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        elapsed < Duration::from_secs(5),
        "expected timeout to bound execution quickly, elapsed: {elapsed:?}, output: {output}"
    );
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt when external command times out, got: {output}"
    );
    assert!(
        output.contains("timed out after 100ms"),
        "expected timeout status in output, got: {output}"
    );
}
