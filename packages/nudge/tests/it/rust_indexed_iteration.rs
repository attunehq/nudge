//! Integration tests for RustIndexedIteration matcher.
//!
//! These tests cover the issue #1 behavior end to end: detect indexing tied to
//! range-based iteration while skipping common legitimate indexing patterns.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

use crate::nudge_binary;

fn setup_config() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(
        dir.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: prefer-enumerate
    description: Prefer enumerate over range indexing
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: RustIndexedIteration
            suggestion: "Use {{ $collection }}.iter().enumerate() instead of indexing {{ $collection }} with {{ $index }}, then retry."
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: RustIndexedIteration
            suggestion: "Use {{ $collection }}.iter().enumerate() instead of indexing {{ $collection }} with {{ $index }}, then retry."
"#,
    )
    .expect("write config");
    dir
}

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

fn assert_interrupt(output: &str) {
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt, got: {output}"
    );
}

fn assert_passthrough(output: &str) {
    assert!(output.is_empty(), "expected passthrough, got: {output}");
}

#[test]
fn detects_for_loop_indexing_over_len_range() {
    let dir = setup_config();
    let input = write_hook(
        "src/main.rs",
        r#"
fn process_items(items: &[String]) {
    for i in 0..items.len() {
        let item = &items[i];
        process(i, item);
    }
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_interrupt(&output);
    assert!(
        output.contains("items.iter().enumerate()"),
        "expected interpolated enumerate suggestion, got: {output}"
    );
}

#[test]
fn detects_range_iterator_closure_indexing() {
    let dir = setup_config();
    let input = write_hook(
        "src/main.rs",
        r#"
fn clone_items(items: &[String]) -> Vec<String> {
    (0..items.len()).map(|i| items[i].clone()).collect()
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_interrupt(&output);
    assert!(
        output.contains("indexing items with i"),
        "expected collection and index captures, got: {output}"
    );
}

#[test]
fn check_command_reports_indexed_iteration() {
    let dir = setup_config();
    let source = dir.path().join("src");
    fs::create_dir(&source).expect("create src dir");
    fs::write(
        source.join("main.rs"),
        r#"
fn process_items(items: &[String]) {
    for i in 0..items.len() {
        process(&items[i]);
    }
}
"#,
    )
    .expect("write source file");

    let output = Command::new(nudge_binary())
        .arg("check")
        .current_dir(dir.path())
        .output()
        .expect("run nudge check");

    pretty_assert_eq!(output.status.code(), Some(1), "expected check failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("src/main.rs:4 [prefer-enumerate]"),
        "expected check output to include file, line, and rule, got: {stdout}"
    );
}

#[test]
fn skips_macro_arguments_and_test_assertions() {
    let dir = setup_config();
    let input = write_hook(
        "src/main.rs",
        r#"
fn assert_items(items: &[String], expected: &[String]) {
    for i in 0..items.len() {
        assert_eq!(items[i], expected[i]);
    }
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_passthrough(&output);
}

#[test]
fn skips_unrelated_literal_indexing() {
    let dir = setup_config();
    let input = write_hook(
        "src/main.rs",
        r#"
fn first_arg(args: &[String]) -> Option<&String> {
    args.get(0).or_else(|| Some(&args[0]))
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_passthrough(&output);
}

#[test]
fn skips_indexing_other_collections_inside_len_loop() {
    let dir = setup_config();
    let input = write_hook(
        "src/main.rs",
        r#"
fn compare(items: &[String], expected: &[String]) {
    for i in 0..items.len() {
        let expected_item = &expected[i];
        process(expected_item);
    }
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_passthrough(&output);
}

#[test]
fn skips_non_zero_start_ranges() {
    let dir = setup_config();
    let input = write_hook(
        "src/main.rs",
        r#"
fn process_tail(items: &[String]) {
    for i in 1..items.len() {
        process(&items[i]);
    }
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_passthrough(&output);
}
