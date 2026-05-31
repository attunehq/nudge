//! Workflow-management integration tests.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use serde_json::Value;
use tempfile::TempDir;

use crate::{nudge_binary, stop_hook, user_prompt_hook};

#[test]
fn workflow_blocks_stop_until_the_agent_confirms_completion() {
    let temp = TempDir::new().expect("temp dir");
    write_workflow_config(&temp);

    let prompt = user_prompt_hook("Fix issue #8 and prove it with tests.");
    let (exit_code, output) = run_codex_hook_in_dir(&temp, &prompt);

    pretty_assert_eq!(exit_code, 0, "prompt hook failed: {output}");
    assert!(
        output.contains("Nudge workflow `issue-resolution` is active"),
        "expected workflow activation context, got: {output}"
    );
    assert!(
        output.contains("NUDGE_WORKFLOW_COMPLETE: issue-resolution"),
        "expected confirmation token guidance, got: {output}"
    );

    let stop = stop_hook("Implemented the change.");
    let (exit_code, output) = run_codex_hook_in_dir(&temp, &stop);

    pretty_assert_eq!(exit_code, 0, "stop hook failed: {output}");
    let json = serde_json::from_str::<Value>(&output).expect("valid stop json");
    pretty_assert_eq!(json["decision"], "block");
    assert!(
        json["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Fix issue #8")
                && reason.contains("Add end-to-end tests")
                && reason.contains("NUDGE_WORKFLOW_COMPLETE: issue-resolution")),
        "expected continuation reason to include prompt, criteria, and token, got: {output}"
    );
}

#[test]
fn workflow_allows_stop_after_the_agent_confirms_completion() {
    let temp = TempDir::new().expect("temp dir");
    write_workflow_config(&temp);

    let prompt = user_prompt_hook("Fix issue #8 and prove it with tests.");
    let (exit_code, output) = run_codex_hook_in_dir(&temp, &prompt);
    pretty_assert_eq!(exit_code, 0, "prompt hook failed: {output}");

    let stop = stop_hook(
        "All requested work is complete.\nNUDGE_WORKFLOW_COMPLETE: issue-resolution\nTests: cargo test -p nudge",
    );
    let (exit_code, output) = run_codex_hook_in_dir(&temp, &stop);

    pretty_assert_eq!(exit_code, 0, "stop hook failed: {output}");
    assert!(
        output.is_empty(),
        "expected confirmed workflow to pass through, got: {output}"
    );

    let (exit_code, output) = run_codex_hook_in_dir(&temp, &stop);
    pretty_assert_eq!(exit_code, 0, "stop hook failed: {output}");
    assert!(
        output.is_empty(),
        "expected confirmed workflow state to be cleared, got: {output}"
    );
}

fn write_workflow_config(temp: &TempDir) {
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
workflows:
  - name: issue-resolution
    description: Complete GitHub issue work before stopping
    prompt:
      - kind: Regex
        pattern: "(?i)issue #8"
    done:
      - "Add end-to-end tests for done and not-done outcomes."
      - "Implement the permanent fix."
      - "Run the relevant tests and report proof."
"#,
    )
    .expect("write config");
}

fn run_codex_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let state_dir = dir.path().join(".nudge-state");
    fs::create_dir_all(&state_dir).expect("create state dir");

    let mut child = Command::new(nudge_binary())
        .args(["codex", "hook"])
        .current_dir(dir.path())
        .env("NUDGE_STATE_DIR", &state_dir)
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
