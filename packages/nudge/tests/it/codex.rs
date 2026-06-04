//! Codex hook integration tests.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

use crate::{
    Expected, assert_expected, codex_apply_patch_hook, nudge_binary, run_codex_hook,
    user_prompt_hook,
};

#[test]
fn codex_apply_patch_write_blocks_inline_imports() {
    let patch = "*** Begin Patch\n*** Add File: test.rs\n+fn main() {\n+    use std::io;\n+}\n*** End Patch\n";
    let input = codex_apply_patch_hook("/tmp", patch);

    let (exit_code, output) = run_codex_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
    assert!(
        !output.contains(r#""continue""#),
        "Codex PreToolUse response must not include unsupported continue field: {output}"
    );
    assert!(
        !output.contains(r#""stopReason""#),
        "Codex PreToolUse response must not include unsupported stopReason field: {output}"
    );
    assert!(
        !output.contains(r#""suppressOutput""#),
        "Codex PreToolUse response must not include unsupported suppressOutput field: {output}"
    );
}

#[test]
fn codex_apply_patch_update_blocks_inline_imports() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(temp.path().join("test.rs"), "fn main() {\n}\n").expect("write file");

    let patch = "*** Begin Patch\n*** Update File: test.rs\n@@\n fn main() {\n+    use std::io;\n }\n*** End Patch\n";
    let input = codex_apply_patch_hook(temp.path().to_str().expect("utf-8 path"), patch);

    let (exit_code, output) = run_codex_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn codex_apply_patch_multi_file_aggregates_matches() {
    let patch = "*** Begin Patch\n*** Add File: first.rs\n+fn first() {\n+    use std::io;\n+}\n*** Add File: second.rs\n+fn second() {\n+    use std::fs;\n+}\n*** End Patch\n";
    let input = codex_apply_patch_hook("/tmp", patch);

    let (exit_code, output) = run_codex_hook(&input);

    assert_expected(exit_code, &output, Expected::Interrupt);
    assert!(
        output.contains("std::io") && output.contains("std::fs"),
        "expected one denial to include matches from both files, got: {output}"
    );
}

#[test]
fn codex_apply_patch_malformed_input_warns_model_and_allows() {
    let patch = "*** Begin Patch\n*** Add File: test.rs\n+fn main() {}\n*** Unsupported Section\n";
    let input = codex_apply_patch_hook("/tmp", patch);

    let (exit_code, output) = run_codex_hook(&input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    assert!(
        json["hookSpecificOutput"]["permissionDecision"] == "allow",
        "expected permissionDecision:allow, got: {output}"
    );
    assert!(
        json["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .is_some_and(|context| context
                .contains("Nudge could not normalize Codex apply_patch input")
                && context.contains("tell the user about this warning")),
        "expected model-visible apply_patch warning, got: {output}"
    );
    assert!(
        json["hookSpecificOutput"].get("updatedInput").is_none(),
        "expected no updated input for warning-only allow, got: {output}"
    );
}

#[test]
fn codex_permission_request_passes_through() {
    let input = serde_json::json!({
        "hook_event_name": "PermissionRequest",
        "cwd": "/tmp",
        "tool_name": "Bash",
        "tool_input": {
            "command": "rm -rf target"
        }
    })
    .to_string();

    let (exit_code, output) = run_codex_hook(&input);

    assert_expected(exit_code, &output, Expected::Passthrough);
}

#[test]
fn codex_user_prompt_submit_injects_plain_text_context() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: dev-server-hint
    description: Help start development server
    message: "Use `cargo run -p nudge -- codex hook` for Codex hook checks."
    on:
      - hook: UserPromptSubmit
        prompt:
          - kind: Regex
            pattern: "(?i)dev server"
"#,
    )
    .expect("write config");
    let input = user_prompt_hook("Can you start the dev server?");

    let (exit_code, output) = run_codex_hook_in_dir(&temp, &input);

    assert_expected(exit_code, &output, Expected::Continue);
    assert!(
        !output.trim_start().starts_with('{'),
        "expected plain text context, got: {output}"
    );
}

#[test]
fn codex_bash_substitution_allows_with_updated_input_and_context() {
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
    let input = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "turn_id": "turn",
        "cwd": temp.path(),
        "tool_name": "Bash",
        "tool_input": {
            "command": "npm install lodash",
            "description": "Install lodash",
            "timeout": 120
        }
    })
    .to_string();

    let (exit_code, output) = run_codex_hook_in_dir(&temp, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    let json = serde_json::from_str::<serde_json::Value>(&output).expect("valid json output");
    assert!(
        json["hookSpecificOutput"]["permissionDecision"] == "allow",
        "expected permissionDecision:allow, got: {output}"
    );
    assert!(
        json["hookSpecificOutput"]["updatedInput"]["command"] == "yarn add lodash",
        "expected rewritten command, got: {output}"
    );
    assert!(
        json["hookSpecificOutput"]["updatedInput"]["description"] == "Install lodash",
        "expected preserved description, got: {output}"
    );
    assert!(
        json["hookSpecificOutput"]["updatedInput"]["timeout"] == 120,
        "expected preserved timeout, got: {output}"
    );
    assert!(
        json["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .is_some_and(|context| context.contains("npm install lodash")
                && context.contains("yarn add lodash")),
        "expected model context describing substitution, got: {output}"
    );
}

fn run_codex_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let mut child = Command::new(nudge_binary())
        .args(["codex", "hook"])
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
