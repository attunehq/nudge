//! Cursor / cursor-agent hook integration tests.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use serde_json::json;
use tempfile::TempDir;

use crate::{Expected, assert_expected, cursor_pretooluse_hook, nudge_binary, run_cursor_hook};

#[test]
fn cursor_write_blocks_inline_imports() {
    let input = cursor_pretooluse_hook(
        "Write",
        json!({
            "path": "test.rs",
            "contents": "fn main() {\n    use std::io;\n}"
        }),
    );

    let (exit_code, output) = run_cursor_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    pretty_assert_eq!(json["permission"], "deny");
    assert!(
        json["agent_message"]
            .as_str()
            .is_some_and(|message| message.contains("Nudge blocked operation")),
        "expected native Cursor deny agent_message, got: {output}"
    );
}

#[test]
fn cursor_write_edit_blocks_inline_imports() {
    let input = cursor_pretooluse_hook(
        "Write",
        json!({
            "file_path": "test.rs",
            "old_string": "fn main() {}",
            "new_string": "fn main() {\n    use std::io;\n}"
        }),
    );

    let (exit_code, output) = run_cursor_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn cursor_write_create_blocks_inline_imports() {
    let input = cursor_pretooluse_hook(
        "Write",
        json!({
            "file_path": "test.rs",
            "old_string": "",
            "new_string": "fn main() {\n    use std::fs;\n}"
        }),
    );

    let (exit_code, output) = run_cursor_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn cursor_shell_blocks_matching_bash_rule() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: no-npm-install
    message: "Use yarn add instead of npm install."
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install"
"#,
    )
    .expect("write config");
    let input = json!({
        "hook_event_name": "preToolUse",
        "conversation_id": "test",
        "cwd": temp.path(),
        "workspace_roots": [temp.path()],
        "tool_name": "Shell",
        "tool_input": {
            "command": "npm install lodash",
            "working_directory": temp.path()
        }
    })
    .to_string();

    let (exit_code, output) = run_cursor_hook_in_dir(&temp, &input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn cursor_before_shell_execution_blocks_matching_bash_rule() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: no-npm-install
    message: "Use yarn add instead of npm install."
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install"
"#,
    )
    .expect("write config");
    let input = json!({
        "hook_event_name": "beforeShellExecution",
        "conversation_id": "test",
        "cwd": temp.path(),
        "command": "npm install lodash",
        "sandbox": false
    })
    .to_string();

    let (exit_code, output) = run_cursor_hook_in_dir(&temp, &input);

    assert_expected(exit_code, &output, Expected::Interrupt);
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    pretty_assert_eq!(json["permission"], "deny");
}

#[test]
fn cursor_web_fetch_blocks_matching_url() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: prefer-local-docs
    message: "Use local docs instead of docs.rs."
    on:
      - hook: PreToolUse
        tool: WebFetch
        url:
          - kind: Regex
            pattern: "docs\\.rs"
"#,
    )
    .expect("write config");
    let input = json!({
        "hook_event_name": "preToolUse",
        "cwd": temp.path(),
        "tool_name": "WebFetch",
        "tool_input": { "url": "https://docs.rs/nudge" }
    })
    .to_string();

    let (exit_code, output) = run_cursor_hook_in_dir(&temp, &input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn cursor_bash_substitution_allows_with_updated_input() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: yarn-add
    description: Use yarn add instead of npm install
    action: substitute
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install(?: (?P<args>.*))?$"
            replace: "yarn add {{ $args }}"
"#,
    )
    .expect("write config");
    let input = json!({
        "hook_event_name": "preToolUse",
        "cwd": temp.path(),
        "tool_name": "Shell",
        "tool_input": {
            "command": "npm install lodash",
            "working_directory": temp.path(),
            "timeout": 120
        }
    })
    .to_string();

    let (exit_code, output) = run_cursor_hook_in_dir(&temp, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    pretty_assert_eq!(json["permission"], "allow");
    pretty_assert_eq!(json["updated_input"]["command"], "yarn add lodash");
    pretty_assert_eq!(json["updated_input"]["timeout"], 120);
    pretty_assert_eq!(
        json["hookSpecificOutput"]["updatedInput"]["command"],
        "yarn add lodash"
    );
}

#[test]
fn cursor_before_shell_execution_substitution_rewrites_command() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: yarn-add
    action: substitute
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install(?: (?P<args>.*))?$"
            replace: "yarn add {{ $args }}"
"#,
    )
    .expect("write config");
    let input = json!({
        "hook_event_name": "beforeShellExecution",
        "cwd": temp.path(),
        "command": "npm install lodash"
    })
    .to_string();

    let (exit_code, output) = run_cursor_hook_in_dir(&temp, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    pretty_assert_eq!(json["permission"], "allow");
    pretty_assert_eq!(json["updated_input"]["command"], "yarn add lodash");
}

#[test]
fn cursor_before_submit_prompt_emits_continue_and_additional_context() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: dev-server-hint
    message: "Use `cargo run -p nudge -- cursor hook` for Cursor hook checks."
    on:
      - hook: UserPromptSubmit
        prompt:
          - kind: Regex
            pattern: "(?i)dev server"
"#,
    )
    .expect("write config");
    let input = json!({
        "hook_event_name": "beforeSubmitPrompt",
        "cwd": temp.path(),
        "prompt": "Can you start the dev server?"
    })
    .to_string();

    let (exit_code, output) = run_cursor_hook_in_dir(&temp, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    pretty_assert_eq!(json["continue"], true);
    pretty_assert_eq!(
        json["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
    assert!(
        json["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .is_some_and(|context| context.contains("cursor hook")),
        "expected Cursor additionalContext JSON, got: {output}"
    );
}

#[test]
fn cursor_claude_compat_payload_still_blocks() {
    let input = json!({
        "hook_event_name": "PreToolUse",
        "cwd": "/tmp",
        "tool_name": "Write",
        "tool_input": {
            "file_path": "test.rs",
            "content": "fn main() {\n    use std::io;\n}"
        }
    })
    .to_string();

    let (exit_code, output) = run_cursor_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn cursor_unrelated_tool_passes_through() {
    let input = cursor_pretooluse_hook(
        "Read",
        json!({
            "path": "src.rs"
        }),
    );

    let (exit_code, output) = run_cursor_hook(&input);

    assert_expected(exit_code, &output, Expected::Passthrough);
}

#[test]
fn cursor_delete_passes_through_without_yaml_matcher() {
    let input = cursor_pretooluse_hook(
        "Delete",
        json!({
            "path": "src.rs"
        }),
    );

    let (exit_code, output) = run_cursor_hook(&input);

    assert_expected(exit_code, &output, Expected::Passthrough);
}

fn run_cursor_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let mut child = Command::new(nudge_binary())
        .args(["cursor", "hook"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn nudge");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");

    let output = child.wait_with_output().expect("wait for nudge");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if !stdout.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };

    (output.status.code().unwrap_or(-1), combined)
}
