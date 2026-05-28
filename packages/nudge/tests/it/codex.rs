//! Codex hook integration tests.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

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
