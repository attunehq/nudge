//! Integration tests for the Rust check-then-unwrap matcher.
//!
//! This matcher catches guard clauses such as `if value.is_none() { return
//! ...; }` followed by `value.unwrap()`, where `let-else` or match-style
//! control flow makes the invariant visible in the type system.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use simple_test_case::test_case;
use tempfile::TempDir;

use crate::nudge_binary;

const CONFIG: &str = r#"
version: 1
rules:
  - name: prefer-let-else-over-check-unwrap
    description: Prefer let-else over checking state and unwrapping later
    message: "Replace the {{ $check_method }} + unwrap guard for `{{ $receiver }}` with let-else or match-style control flow, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: RustCheckThenUnwrap
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: RustCheckThenUnwrap
"#;

fn setup_config() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join(".nudge.yaml"), CONFIG).expect("write config");
    dir
}

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

fn run_agent_hook(dir: &TempDir, input: &str) -> (i32, String) {
    let mut child = Command::new(nudge_binary())
        .args(["claude", "hook"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn nudge");

    child
        .stdin
        .as_mut()
        .expect("failed to get stdin")
        .write_all(input.as_bytes())
        .expect("failed to write hook input");

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

fn run_check(dir: &TempDir, source: &str) -> (i32, String, String) {
    let source_path = dir.path().join("src/lib.rs");
    fs::create_dir_all(source_path.parent().expect("source has parent"))
        .expect("create source dir");
    fs::write(&source_path, source).expect("write source");

    let output = Command::new(nudge_binary())
        .args(["check", "src/lib.rs"])
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run nudge check");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn assert_interrupt(output: &str, expected_check_method: &str, expected_receiver: &str) {
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt, got: {output}"
    );
    assert!(
        output.contains(expected_check_method),
        "expected check method `{expected_check_method}` in output, got: {output}"
    );
    assert!(
        output.contains(expected_receiver),
        "expected receiver `{expected_receiver}` in output, got: {output}"
    );
}

#[test_case(
    r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if value.is_none() {
        return Err("missing".to_string());
    }
    let value = value.unwrap();
    Ok(value)
}
"#,
    "is_none",
    "value";
    "option is_none guard then unwrap"
)]
#[test_case(
    r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if !value.is_some() {
        return Err("missing".to_string());
    }
    let value = value.unwrap();
    Ok(value)
}
"#,
    "is_some",
    "value";
    "negated option is_some guard then unwrap"
)]
#[test_case(
    r#"
fn parse(result: Result<String, String>) -> String {
    if result.is_err() {
        return handle_error();
    }
    let value = result.unwrap();
    value
}
"#,
    "is_err",
    "result";
    "result is_err guard then unwrap"
)]
#[test_case(
    r#"
fn parse(result: Result<String, String>) -> String {
    if !result.is_ok() {
        return handle_error();
    }
    let value = result.unwrap();
    value
}
"#,
    "is_ok",
    "result";
    "negated result is_ok guard then unwrap"
)]
#[test]
fn write_hook_detects_check_then_unwrap(
    source: &str,
    expected_check_method: &str,
    expected_receiver: &str,
) {
    let dir = setup_config();
    let input = write_hook("src/lib.rs", source);
    let (exit_code, output) = run_agent_hook(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_interrupt(&output, expected_check_method, expected_receiver);
}

#[test]
fn edit_hook_detects_check_then_unwrap() {
    let dir = setup_config();
    let source = r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if value.is_none() {
        return Err("missing".to_string());
    }
    let value = value.unwrap();
    Ok(value)
}
"#;
    let input = edit_hook("src/lib.rs", "", source);
    let (exit_code, output) = run_agent_hook(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_interrupt(&output, "is_none", "value");
}

#[test_case(
    r#"
fn parse(value: Option<String>, fallback: Option<String>) -> Result<String, String> {
    if value.is_none() {
        return Err("missing".to_string());
    }
    let value = fallback.unwrap();
    Ok(value)
}
"#;
    "different unwrap receiver"
)]
#[test_case(
    r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if value.is_none() {
        log::warn!("missing");
    }
    let value = value.unwrap();
    Ok(value)
}
"#;
    "guard does not exit"
)]
#[test_case(
    r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if value.is_none() {
        return Err("missing".to_string());
    } else {
        log::debug!("present");
    }
    let value = value.unwrap();
    Ok(value)
}
"#;
    "if has else"
)]
#[test_case(
    r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if value.is_none() {
        return Err("missing".to_string());
    }
    audit();
    let value = value.unwrap();
    Ok(value)
}
"#;
    "executable statement between guard and unwrap"
)]
#[test_case(
    r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if value.is_some() {
        return Err("unexpected".to_string());
    }
    let value = value.unwrap();
    Ok(value)
}
"#;
    "guard exits on present value"
)]
#[test]
fn write_hook_avoids_false_positives(source: &str) {
    let dir = setup_config();
    let input = write_hook("src/lib.rs", source);
    let (exit_code, output) = run_agent_hook(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for false positive case, got: {output}"
    );
}

#[test]
fn write_hook_allows_comment_between_guard_and_unwrap() {
    let dir = setup_config();
    let source = r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if value.is_none() {
        return Err("missing".to_string());
    }
    // Explains why absence returns early.
    let value = value.unwrap();
    Ok(value)
}
"#;
    let input = write_hook("src/lib.rs", source);
    let (exit_code, output) = run_agent_hook(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_interrupt(&output, "is_none", "value");
}

#[test]
fn check_command_reports_check_then_unwrap() {
    let dir = setup_config();
    let source = r#"
fn parse(value: Option<String>) -> Result<String, String> {
    if value.is_none() {
        return Err("missing".to_string());
    }
    let value = value.unwrap();
    Ok(value)
}
"#;
    let (exit_code, stdout, stderr) = run_check(&dir, source);

    pretty_assert_eq!(exit_code, 1, "expected check failure, stderr: {stderr}");
    assert!(
        stdout.contains("src/lib.rs:3 [prefer-let-else-over-check-unwrap]"),
        "expected issue on guard line, got stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("is_none") && stdout.contains("value"),
        "expected interpolated captures in check output, got: {stdout}"
    );
}
