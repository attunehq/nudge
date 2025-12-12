//! Integration tests for the user-defined rules system.
//!
//! These tests verify that the YAML-based rule system works correctly:
//! - Rules are loaded from .nudge.yaml
//! - Rules match based on hook type, tool name, file pattern, and content
//! - Rules produce correct responses (interrupt vs continue vs passthrough)

mod basic;
mod bash;
mod cli;
mod edit_tool;
mod external;
mod inline_imports;
mod message_content;
mod multiple_rules;
mod non_rust_files;
mod syntax_tree;
mod user_prompt;
mod webfetch;

use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use xshell::Shell;

/// Expected outcome from running a hook through nudge.
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

/// Build a PreToolUse hook JSON payload for WebFetch tool.
pub fn webfetch_hook(url: &str, prompt: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "tool_name": "WebFetch",
        "tool_use_id": "123",
        "tool_input": {
            "url": url,
            "prompt": prompt
        }
    })
    .to_string()
}

/// Build a PreToolUse hook JSON payload for Bash tool.
pub fn bash_hook(command: &str) -> String {
    bash_hook_with_cwd(command, "/tmp")
}

/// Build a PreToolUse hook JSON payload for Bash tool with custom cwd.
pub fn bash_hook_with_cwd(command: &str, cwd: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": cwd,
        "tool_name": "Bash",
        "tool_use_id": "123",
        "tool_input": {
            "command": command,
            "description": "Test command"
        }
    })
    .to_string()
}

/// Run nudge claude hook with the given input JSON and return (exit_code, output).
pub fn run_hook(_sh: &Shell, input: &str) -> (i32, String) {
    // Build and get the binary path
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "nudge"])
        .status()
        .expect("failed to build nudge");
    assert!(status.success(), "cargo build failed");

    // Run the binary directly with stdin
    let mut child = Command::new("cargo")
        .args(["run", "--quiet", "-p", "nudge", "--", "claude", "hook"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn nudge");

    // Write input to stdin
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

    // For interrupt responses, output goes to stderr
    // For continue responses, output goes to stdout
    // cargo -q suppresses compiler warnings, so we can use output directly
    let combined = if !stdout.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };

    (exit_code, combined)
}

pub fn assert_expected(exit_code: i32, output: &str, expected: Expected) {
    match expected {
        Expected::Passthrough => {
            pretty_assert_eq!(
                exit_code,
                0,
                "expected passthrough (exit 0), output: {output}"
            );
            assert!(
                output.is_empty(),
                "expected no output for passthrough, got: {output}"
            );
        }
        Expected::Continue => {
            pretty_assert_eq!(exit_code, 0, "expected continue (exit 0)");
            assert!(
                !output.is_empty(),
                "expected non-empty output for continue, got nothing"
            );
            // Continue responses are plain text for UserPromptSubmit
        }
        Expected::Interrupt => {
            pretty_assert_eq!(
                exit_code,
                0,
                "expected interrupt (exit 0), output: {output}"
            );
            assert!(
                output.contains(r#""permissionDecision":"deny""#),
                "expected permissionDecision:deny in output, got: {output}"
            );
        }
    }
}

/// Run a nudge subcommand and return (exit_code, stdout, stderr).
pub fn run_nudge(args: &[&str]) -> (i32, String, String) {
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "nudge"])
        .status()
        .expect("failed to build nudge");
    assert!(status.success(), "cargo build failed");

    let mut cmd_args = vec!["run", "--quiet", "-p", "nudge", "--"];
    cmd_args.extend(args);

    let output = Command::new("cargo")
        .args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}
