//! Grok Build hook integration tests.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use serde_json::json;
use tempfile::TempDir;

use crate::{Expected, assert_expected, grok_pretooluse_hook, nudge_binary, run_grok_hook};

#[test]
fn grok_write_file_blocks_inline_imports() {
    let input = grok_pretooluse_hook(
        "write_file",
        json!({
            "path": "test.rs",
            "content": "fn main() {\n    use std::io;\n}"
        }),
    );

    let (exit_code, output) = run_grok_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    pretty_assert_eq!(json["decision"], "deny");
    assert!(
        json["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Nudge blocked operation")),
        "expected native Grok deny reason, got: {output}"
    );
}

#[test]
fn grok_search_replace_edit_blocks_inline_imports() {
    let input = grok_pretooluse_hook(
        "search_replace",
        json!({
            "file_path": "test.rs",
            "old_string": "fn main() {}",
            "new_string": "fn main() {\n    use std::io;\n}"
        }),
    );

    let (exit_code, output) = run_grok_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn grok_search_replace_create_blocks_inline_imports() {
    let input = grok_pretooluse_hook(
        "search_replace",
        json!({
            "file_path": "test.rs",
            "old_string": "",
            "new_string": "fn main() {\n    use std::fs;\n}"
        }),
    );

    let (exit_code, output) = run_grok_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn grok_run_terminal_command_blocks_matching_bash_rule() {
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
        "hookEventName": "pre_tool_use",
        "sessionId": "test",
        "cwd": temp.path(),
        "workspaceRoot": temp.path(),
        "toolName": "run_terminal_command",
        "toolInput": {
            "command": "npm install lodash",
            "description": "Install lodash"
        }
    })
    .to_string();

    let (exit_code, output) = run_grok_hook_in_dir(&temp, &input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn grok_web_fetch_blocks_matching_url() {
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
        "hookEventName": "PreToolUse",
        "cwd": temp.path(),
        "toolName": "web_fetch",
        "toolInput": { "url": "https://docs.rs/nudge" }
    })
    .to_string();

    let (exit_code, output) = run_grok_hook_in_dir(&temp, &input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn grok_bash_substitution_allows_with_updated_input() {
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
        "hookEventName": "pre_tool_use",
        "cwd": temp.path(),
        "toolName": "run_terminal_command",
        "toolInput": {
            "command": "npm install lodash",
            "description": "Install lodash",
            "timeout": 120
        }
    })
    .to_string();

    let (exit_code, output) = run_grok_hook_in_dir(&temp, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    pretty_assert_eq!(json["decision"], "allow");
    pretty_assert_eq!(
        json["hookSpecificOutput"]["updatedInput"]["command"],
        "yarn add lodash"
    );
    pretty_assert_eq!(
        json["hookSpecificOutput"]["updatedInput"]["description"],
        "Install lodash"
    );
    pretty_assert_eq!(json["hookSpecificOutput"]["updatedInput"]["timeout"], 120);
}

#[test]
fn grok_user_prompt_submit_emits_additional_context_json() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: dev-server-hint
    message: "Use `cargo run -p nudge -- grok hook` for Grok hook checks."
    on:
      - hook: UserPromptSubmit
        prompt:
          - kind: Regex
            pattern: "(?i)dev server"
"#,
    )
    .expect("write config");
    let input = json!({
        "hookEventName": "user_prompt_submit",
        "cwd": temp.path(),
        "prompt": "Can you start the dev server?"
    })
    .to_string();

    let (exit_code, output) = run_grok_hook_in_dir(&temp, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    pretty_assert_eq!(
        json["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
    assert!(
        json["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .is_some_and(|context| context.contains("grok hook")),
        "expected Grok additionalContext JSON, got: {output}"
    );
}

#[test]
fn grok_snake_case_claude_compat_payload_still_blocks() {
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

    let (exit_code, output) = run_grok_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn grok_unrelated_tool_passes_through() {
    let input = grok_pretooluse_hook(
        "read_file",
        json!({
            "target_file": "src.rs"
        }),
    );

    let (exit_code, output) = run_grok_hook(&input);

    assert_expected(exit_code, &output, Expected::Passthrough);
}

#[test]
fn grok_permission_request_passes_through() {
    let input = json!({
        "hook_event_name": "PermissionRequest",
        "cwd": "/tmp",
        "tool_name": "Bash",
        "tool_input": { "command": "rm -rf target" }
    })
    .to_string();

    let (exit_code, output) = run_grok_hook(&input);

    assert_expected(exit_code, &output, Expected::Passthrough);
}

fn run_grok_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let mut child = Command::new(nudge_binary())
        .args(["grok", "hook"])
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
