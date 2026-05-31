//! Integration tests for RustFunctionalMutation matcher.
//!
//! These tests exercise the full hook path for the Rust-specific matcher that
//! catches simple mutation-heavy loops with clear iterator equivalents.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

use crate::nudge_binary;

fn setup_config() -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join(".nudge.yaml");
    fs::write(
        &config_path,
        r#"
version: 1
rules:
  - name: prefer-functional-mutation
    description: Prefer iterator adapters over simple mutable loop accumulation
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: RustFunctionalMutation
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: RustFunctionalMutation
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

#[track_caller]
fn assert_interrupt(output: &str) {
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt, got: {output}"
    );
}

#[track_caller]
fn assert_passthrough(output: &str) {
    assert!(
        output.is_empty(),
        "expected passthrough with no output, got: {output}"
    );
}

#[test]
fn detects_vec_push_collection_loop() {
    let dir = setup_config();
    let input = write_hook(
        "test.rs",
        r#"
fn build(items: Vec<Item>) -> Vec<Value> {
    let mut results = Vec::new();
    for item in items {
        results.push(process(item));
    }
    results
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected hook success, output: {output}");
    assert_interrupt(&output);
    assert!(
        output.contains("map") && output.contains("collect::<Vec<_>>()"),
        "expected map/collect suggestion, got: {output}"
    );
}

#[test]
fn detects_filter_map_loop() {
    let dir = setup_config();
    let input = write_hook(
        "test.rs",
        r#"
fn build(items: Vec<Item>) -> Vec<Value> {
    let mut results = Vec::new();
    for item in items {
        if let Some(value) = process(item) {
            results.push(value);
        }
    }
    results
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected hook success, output: {output}");
    assert_interrupt(&output);
    assert!(
        output.contains("filter_map") && output.contains("collect::<Vec<_>>()"),
        "expected filter_map suggestion, got: {output}"
    );
}

#[test]
fn detects_find_loop() {
    let dir = setup_config();
    let input = write_hook(
        "test.rs",
        r#"
fn find_match(items: Vec<Item>) -> Option<Item> {
    let mut found = None;
    for item in items {
        if item.is_ready() {
            found = Some(item);
            break;
        }
    }
    found
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected hook success, output: {output}");
    assert_interrupt(&output);
    assert!(
        output.contains("find") || output.contains("find_map"),
        "expected find suggestion, got: {output}"
    );
}

#[test]
fn detects_fold_loop() {
    let dir = setup_config();
    let input = write_hook(
        "test.rs",
        r#"
fn sum(items: Vec<i64>) -> i64 {
    let mut total = 0;
    for item in items {
        total = combine(total, item);
    }
    total
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected hook success, output: {output}");
    assert_interrupt(&output);
    assert!(
        output.contains("fold"),
        "expected fold suggestion, got: {output}"
    );
}

#[test]
fn skips_vec_with_capacity_for_performance_sensitive_collection() {
    let dir = setup_config();
    let input = write_hook(
        "test.rs",
        r#"
fn build(items: &[Item]) -> Vec<Value> {
    let mut results = Vec::with_capacity(items.len());
    for item in items {
        results.push(process(item));
    }
    results
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected hook success, output: {output}");
    assert_passthrough(&output);
}

#[test]
fn skips_loops_with_extra_side_effects() {
    let dir = setup_config();
    let input = write_hook(
        "test.rs",
        r#"
fn build(items: Vec<Item>) -> Vec<Value> {
    let mut results = Vec::new();
    for item in items {
        metrics.count_item();
        results.push(process(item));
    }
    results
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected hook success, output: {output}");
    assert_passthrough(&output);
}

#[test]
fn skips_io_style_mutation() {
    let dir = setup_config();
    let input = write_hook(
        "test.rs",
        r#"
use std::io::{Result, Write};

fn write_all(items: Vec<Item>) -> Result<Vec<u8>> {
    let mut writer = Vec::new();
    for item in items {
        writeln!(writer, "{}", item)?;
    }
    Ok(writer)
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected hook success, output: {output}");
    assert_passthrough(&output);
}

#[test]
fn skips_unsafe_or_ffi_contexts() {
    let dir = setup_config();
    let input = write_hook(
        "test.rs",
        r#"
fn collect_raw(items: Vec<Item>) -> Vec<Value> {
    unsafe {
        let mut results = Vec::new();
        for item in items {
            results.push(from_raw(item));
        }
        results
    }
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected hook success, output: {output}");
    assert_passthrough(&output);
}
