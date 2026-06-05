//! Integration tests for Markdown code-block file targets.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

use crate::nudge_binary;

const RUST_LHS_TYPE_RULES: &str = r#"
version: 1
rules:
  - name: no-rust-lhs-type-in-markdown
    description: Use inferred local types in Rust examples
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.md"
        target:
          kind: MarkdownCodeBlock
          language: rust
        content:
          - kind: SyntaxTree
            language: rust
            query: "(let_declaration pattern: (identifier) @binding type: (_) @type)"
            suggestion: "Remove {{ $type }} from {{ $binding }} in this Markdown Rust block."
      - hook: PreToolUse
        tool: Edit
        file: "**/*.md"
        target:
          kind: MarkdownCodeBlock
          language: rust
        new_content:
          - kind: SyntaxTree
            language: rust
            query: "(let_declaration pattern: (identifier) @binding type: (_) @type)"
            suggestion: "Remove {{ $type }} from {{ $binding }} in this Markdown Rust block."
"#;

fn setup_config(rules_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(dir.path().join(".nudge.yaml"), rules_yaml).expect("write config");
    dir
}

fn run_hook_in_dir(dir: &TempDir, input: String) -> (i32, String) {
    let mut child = Command::new(nudge_binary())
        .args(["claude", "hook"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run nudge hook");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write hook input");

    let output = child.wait_with_output().expect("wait for nudge hook");
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        stdout.trim().to_string()
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
        .expect("run nudge");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn write_hook(cwd: &str, file_path: &str, content: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": cwd,
        "tool_name": "Write",
        "tool_use_id": "123",
        "tool_input": {
            "file_path": file_path,
            "content": content
        }
    })
    .to_string()
}

fn edit_hook(cwd: &str, file_path: &str, old_string: &str, new_string: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": cwd,
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

#[test]
fn markdown_code_block_target_blocks_write_inside_matching_language_fence() {
    let dir = setup_config(RUST_LHS_TYPE_RULES);
    let markdown = r#"# Guide

This prose says `let fake: usize = 1;` but should not matter.

```python
value: int = 1
```

```rust
fn demo() {
    let value: usize = 1;
}
```
"#;

    let input = write_hook(&dir.path().display().to_string(), "docs/guide.md", markdown);
    let (exit_code, output) = run_hook_in_dir(&dir, input);

    pretty_assert_eq!(exit_code, 0, "expected hook command success: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected Markdown Rust block denial, got: {output}"
    );
    assert!(
        output.contains("Remove usize from value in this Markdown Rust block."),
        "expected capture interpolation from Rust code block, got: {output}"
    );
    assert!(
        output.contains("let value: usize = 1;"),
        "expected snippet from physical Markdown content, got: {output}"
    );
}

#[test]
fn markdown_code_block_target_reconstructs_claude_edit_context() {
    let dir = setup_config(RUST_LHS_TYPE_RULES);
    fs::create_dir_all(dir.path().join("docs")).expect("create docs dir");
    fs::write(
        dir.path().join("docs/guide.md"),
        r#"# Guide

```rust
fn demo() {
    let value = 1;
}
```
"#,
    )
    .expect("write markdown");

    let input = edit_hook(
        &dir.path().display().to_string(),
        "docs/guide.md",
        "let value = 1;",
        "let value: usize = 1;",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, input);

    pretty_assert_eq!(exit_code, 0, "expected hook command success: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected denial after reconstructing Markdown edit context, got: {output}"
    );
    assert!(
        output.contains("let value: usize = 1;"),
        "expected snippet from reconstructed Markdown file, got: {output}"
    );
}

#[test]
fn markdown_code_block_target_reports_physical_lines_in_check_mode() {
    let dir = setup_config(RUST_LHS_TYPE_RULES);
    fs::create_dir_all(dir.path().join("docs")).expect("create docs dir");
    fs::write(
        dir.path().join("docs/guide.md"),
        "# Guide\n\nText.\n\n```rust\nfn demo() {\n    let value: usize = 1;\n}\n```\n",
    )
    .expect("write markdown");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "docs/guide.md"]);

    pretty_assert_eq!(exit_code, 1, "expected check failure, stderr: {stderr}");
    assert!(
        stdout.contains("docs/guide.md:7 [no-rust-lhs-type-in-markdown]"),
        "expected physical Markdown line for Rust block issue, got: {stdout}"
    );
    assert!(
        stdout.contains("Remove usize from value in this Markdown Rust block."),
        "expected interpolated check message, got: {stdout}"
    );
}

#[test]
fn explicit_content_target_preserves_raw_content_matching() {
    let dir = setup_config(
        r#"
version: 1
rules:
  - name: raw-content-target
    message: "Raw content matched."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.md"
        target:
          kind: Content
        content:
          - kind: Regex
            pattern: "TODO"
"#,
    );

    let input = write_hook(
        &dir.path().display().to_string(),
        "docs/guide.md",
        "TODO outside any code block\n",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, input);

    pretty_assert_eq!(exit_code, 0, "expected hook command success: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected explicit Content target to match raw Markdown content, got: {output}"
    );
}
