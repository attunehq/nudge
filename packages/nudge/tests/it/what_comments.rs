//! Integration tests for the WhatComment matcher.
//!
//! These tests exercise the hook and `nudge check` paths so obvious "what"
//! comments are caught without blocking comments that explain reasoning.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::{codex_apply_patch_hook, edit_hook, nudge_binary, write_hook};
use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

fn setup_config(rules_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join(".nudge.yaml"), rules_yaml).expect("write config");
    dir
}

fn write_rule() -> &'static str {
    r#"
version: 1
rules:
  - name: no-what-comments
    description: Comments should explain why, not restate obvious code
    message: "Comment restates the next code: {{ $comment }}. Remove it or explain why, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: WhatComment
            language: rust
"#
}

fn edit_rule() -> &'static str {
    r#"
version: 1
rules:
  - name: no-what-comments
    description: Comments should explain why, not restate obvious code
    message: "Comment restates the next code: {{ $comment }}. Remove it or explain why, then retry."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: WhatComment
            language: rust
"#
}

fn run_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    run_agent_hook_in_dir("claude", dir, input)
}

fn run_codex_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    run_agent_hook_in_dir("codex", dir, input)
}

fn run_agent_hook_in_dir(agent: &str, dir: &TempDir, input: &str) -> (i32, String) {
    let mut child = Command::new(nudge_binary())
        .args([agent, "hook"])
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if !stdout.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };

    (output.status.code().unwrap_or(-1), combined)
}

fn assert_denied(exit_code: i32, output: &str, comment: &str) {
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt, got: {output}"
    );
    assert!(
        output.contains(comment),
        "expected output to mention {comment:?}, got: {output}"
    );
}

fn assert_passed(exit_code: i32, output: &str) {
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(output.is_empty(), "expected passthrough, got: {output}");
}

#[test]
fn detects_obvious_what_comments_on_write() {
    let dir = setup_config(write_rule());

    let cases = [
        (
            "rename call",
            "// Rename the temp file to the target path\n\
             tokio::fs::rename(temp_path, target).await?;\n",
            "Rename the temp file to the target path",
        ),
        (
            "loop body",
            "// Loop through items and process each one\n\
             for item in items { process(item); }\n",
            "Loop through items and process each one",
        ),
        (
            "assignment",
            "// Set x to 5\n\
             let x = 5;\n",
            "Set x to 5",
        ),
        (
            "function call",
            "// Call the process function\n\
             process(item);\n",
            "Call the process function",
        ),
    ];

    for (_name, content, comment) in cases {
        let input = write_hook("src/lib.rs", content);
        let (exit_code, output) = run_hook_in_dir(&dir, &input);
        assert_denied(exit_code, &output, comment);
    }
}

#[test]
fn detects_obvious_what_comments_on_codex_write() {
    let dir = setup_config(write_rule());
    let patch = "*** Begin Patch\n*** Add File: src/lib.rs\n+// Set retries to 3\n+let retries = 3;\n*** End Patch\n";
    let input = codex_apply_patch_hook(dir.path().to_str().expect("utf-8 path"), patch);

    let (exit_code, output) = run_codex_hook_in_dir(&dir, &input);
    assert_denied(exit_code, &output, "Set retries to 3");
}

#[test]
fn detects_obvious_what_comments_on_edit() {
    let dir = setup_config(edit_rule());
    let input = edit_hook("src/lib.rs", "", "// Set retries to 3\nlet retries = 3;\n");

    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    assert_denied(exit_code, &output, "Set retries to 3");
}

#[test]
fn preserves_comments_that_explain_why() {
    let dir = setup_config(write_rule());

    let cases = [
        "// Use atomic rename to prevent partial reads during concurrent access\n\
         tokio::fs::rename(temp_path, target).await?;\n",
        "// Process in serial to avoid overwhelming the database connection pool\n\
         for item in items { process(item); }\n",
        "// SAFETY: ptr is non-null because allocator returned Ok above\n\
         unsafe { ptr.write(value); }\n",
        "// Keep this branch for backwards-compatible config migrations\n\
         if legacy_config.is_some() { migrate(); }\n",
    ];

    for content in cases {
        let input = write_hook("src/lib.rs", content);
        let (exit_code, output) = run_hook_in_dir(&dir, &input);
        assert_passed(exit_code, &output);
    }
}

#[test]
fn preserves_non_adjacent_and_doc_comments() {
    let dir = setup_config(write_rule());

    let cases = [
        "/// Rename the temp file to the target path\n\
         pub fn rename_temp() {}\n",
        "// Rename the temp file to the target path\n\
         \n\
         tokio::fs::rename(temp_path, target).await?;\n",
        "// TODO: Set retries from config\n\
         let retries = 3;\n",
    ];

    for content in cases {
        let input = write_hook("src/lib.rs", content);
        let (exit_code, output) = run_hook_in_dir(&dir, &input);
        assert_passed(exit_code, &output);
    }
}

#[test]
fn check_reports_the_comment_line() {
    let dir = setup_config(write_rule());
    let source_dir = dir.path().join("src");
    fs::create_dir(&source_dir).expect("create src dir");
    fs::write(
        source_dir.join("lib.rs"),
        "fn rename() {\n    // Rename the temp file to the target path\n    tokio::fs::rename(temp_path, target).await?;\n}\n",
    )
    .expect("write source file");

    let output = Command::new(nudge_binary())
        .args(["check", "src/lib.rs"])
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run nudge check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    pretty_assert_eq!(
        output.status.code().unwrap_or(-1),
        1,
        "expected check failure, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("src/lib.rs:2 [no-what-comments]"),
        "expected issue on comment line, got stdout: {stdout}"
    );
    assert!(
        stdout.contains("Rename the temp file to the target path"),
        "expected captured comment in check output, got stdout: {stdout}"
    );
}
