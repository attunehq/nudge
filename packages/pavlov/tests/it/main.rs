//! Integration tests for the user-defined rules system.
//!
//! These tests verify that the YAML-based rule system works correctly:
//! - Rules are loaded from .pavlov.yaml
//! - Rules match based on hook type, tool name, file pattern, and content
//! - Rules produce correct responses (interrupt vs continue vs passthrough)

mod basic;
mod cli;
mod edit_tool;
mod inline_imports;
mod message_content;
mod multiple_rules;
mod non_rust_files;
mod user_prompt;

use pretty_assertions::assert_eq as pretty_assert_eq;
use xshell::Shell;

/// Expected outcome from running a hook through pavlov.
#[derive(Debug, Clone, PartialEq)]
pub enum Expected {
    /// Passthrough: exit 0, no output
    Passthrough,

    /// Continue: exit 0, output contains "continue":true
    #[allow(dead_code)]
    Continue,

    /// Interrupt: exit 2, output contains "continue":false
    Interrupt,
}

/// Build a PreToolUse hook JSON payload for Write tool.
pub fn write_hook(file_path: &str, content: &str) -> String {
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
pub fn edit_hook(file_path: &str, old_string: &str, new_string: &str) -> String {
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
pub fn user_prompt_hook(prompt: &str) -> String {
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
pub fn run_hook(_sh: &Shell, input: &str) -> (i32, String) {
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
        stdin
            .write_all(input.as_bytes())
            .expect("failed to write to stdin");
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

pub fn assert_expected(exit_code: i32, output: &str, expected: Expected) {
    match expected {
        Expected::Passthrough => {
            pretty_assert_eq!(exit_code, 0, "expected passthrough (exit 0)");
            assert!(
                output.is_empty(),
                "expected no output for passthrough, got: {output}"
            );
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
                exit_code,
                2,
                "expected interrupt (exit 2), output: {output}"
            );
            assert!(
                output.contains(r#""continue":false"#),
                "expected continue:false in output, got: {output}"
            );
        }
    }
}

/// Run a pavlov subcommand and return (exit_code, stdout, stderr).
pub fn run_pavlov(args: &[&str]) -> (i32, String, String) {
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
