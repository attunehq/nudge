//! Integration tests for Rust stuttering type-name detection.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

use crate::nudge_binary;

fn setup_config(rules_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join(".nudge.yaml"), rules_yaml).expect("write config");
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

fn run_nudge_in_dir(dir: &TempDir, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(nudge_binary())
        .args(args)
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
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

fn stuttering_rule() -> &'static str {
    r#"
version: 1
rules:
  - name: stuttering-types
    description: Avoid repeating module context in Rust type names
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: StutteringTypeName
            language: rust
            redundant_suffixes: ["Manager", "Service", "Handler"]
            module_aliases:
              db: ["Database"]
            allow:
              - "storage::StorageEngine"
            suggestion: "Rename `{{ $type }}` to `{{ $replacement }}`; {{ $reason }}."
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: StutteringTypeName
            language: rust
            redundant_suffixes: ["Manager", "Service", "Handler"]
            module_aliases:
              db: ["Database"]
            allow:
              - "storage::StorageEngine"
            suggestion: "Rename `{{ $type }}` to `{{ $replacement }}`; {{ $reason }}."
"#
}

#[test]
fn test_stuttering_type_names_detect_inline_modules() {
    let dir = setup_config(stuttering_rule());
    let input = write_hook(
        "src/lib.rs",
        r#"
mod storage {
    pub struct CasStorage;
    pub enum DiskStorage {}
    pub struct StorageEngine;
}

mod cache {
    pub type KeyCache = String;
}

mod auth {
    pub struct JwtManager;
}

mod db {
    pub struct Database;
}
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for stuttering type names, got: {output}"
    );
    assert!(output.contains("Rename `CasStorage` to `Cas`"));
    assert!(output.contains("Rename `DiskStorage` to `Disk`"));
    assert!(output.contains("Rename `KeyCache` to `Key`"));
    assert!(output.contains("Rename `JwtManager` to `Jwt`"));
    assert!(output.contains("module `db` already provides `Database`"));
    assert!(
        !output.contains("Rename `StorageEngine`"),
        "allow-list should suppress intentional repetitions, got: {output}"
    );
}

#[test]
fn test_stuttering_type_names_use_file_module_context() {
    let dir = setup_config(stuttering_rule());
    let input = write_hook("src/storage.rs", "pub struct CasStorage;\n");

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for file module stutter, got: {output}"
    );
    assert!(output.contains("Rename `CasStorage` to `Cas`"));
}

#[test]
fn test_stuttering_type_names_check_command_reports_lines() {
    let dir = setup_config(stuttering_rule());
    let src = dir.path().join("src");
    fs::create_dir(&src).expect("create src");
    fs::write(src.join("storage.rs"), "pub struct CasStorage;\n").expect("write source");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "src/storage.rs"]);

    assert_ne!(exit_code, 0, "check should fail for violations");
    assert!(stderr.is_empty(), "expected no stderr, got: {stderr}");
    assert!(
        stdout.contains("src/storage.rs:1 [stuttering-types]"),
        "expected file/line/rule output, got: {stdout}"
    );
    assert!(stdout.contains("Rename `CasStorage` to `Cas`"));
}

#[test]
fn test_stuttering_type_names_edit_uses_replacement_content() {
    let dir = setup_config(stuttering_rule());
    let input = edit_hook("src/auth.rs", "pub struct Jwt;", "pub struct JwtManager;");

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for edit replacement, got: {output}"
    );
    assert!(output.contains("Rename `JwtManager` to `Jwt`"));
}

#[test]
fn test_stuttering_type_names_pass_clear_names_and_allowed_repetitions() {
    let dir = setup_config(stuttering_rule());
    let input = write_hook(
        "src/storage.rs",
        r#"
pub struct Cas;
pub struct Disk;
pub struct StorageEngine;
"#,
    );

    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for clear or allowed names, got: {output}"
    );
}
