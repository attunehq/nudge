//! UserPromptSubmit Hook Tests

use std::{
    fs,
    io::Write as _,
    process::{Command, Stdio},
};

use crate::{Expected, assert_expected, nudge_binary, run_hook, user_prompt_hook};
use tempfile::TempDir;
use xshell::Shell;

#[test]
fn test_user_prompt_no_matching_rules() {
    let sh = Shell::new().expect("create shell");
    let input = user_prompt_hook("hello world");
    let (exit_code, output) = run_hook(&sh, &input);
    // No UserPromptSubmit rules in the test config, so should passthrough
    assert_expected(exit_code, &output, Expected::Passthrough);
}

#[test]
fn semantic_prompt_injects_after_matching_project_file_change() {
    let temp = project_with_semantic_prompt_rule();
    let input = user_prompt_hook_in_dir(temp.path(), "try executing it");

    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);

    let input = write_hook_in_dir(
        temp.path(),
        "packages/hurry/src/daemon.rs",
        "pub fn sync() {}\n",
    );
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);

    let input = user_prompt_hook_in_dir(temp.path(), "try executing it");
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Continue);
    assert!(
        output.contains("Use `hurry-dev` after `make install-dev`."),
        "expected local testing reminder, got: {output}"
    );
}

#[test]
fn semantic_prompt_requires_matching_file_change() {
    let temp = project_with_semantic_prompt_rule();
    let input = write_hook_in_dir(temp.path(), "README.md", "# docs\n");
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);

    let input = user_prompt_hook_in_dir(temp.path(), "let's test this");
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}

#[test]
fn semantic_prompt_rejects_unrelated_prompt_after_file_change() {
    let temp = project_with_semantic_prompt_rule();
    let input = write_hook_in_dir(
        temp.path(),
        "packages/hurry/src/daemon.rs",
        "pub fn sync() {}\n",
    );
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);

    let input = user_prompt_hook_in_dir(temp.path(), "explain how this daemon is structured");
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}

#[test]
fn semantic_prompt_once_per_change_suppresses_repeated_noise() {
    let temp = project_with_semantic_prompt_rule();
    let input = write_hook_in_dir(
        temp.path(),
        "packages/hurry/src/daemon.rs",
        "pub fn sync() {}\n",
    );
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);

    let input = user_prompt_hook_in_dir(temp.path(), "does this work?");
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Continue);

    let input = user_prompt_hook_in_dir(temp.path(), "try running it");
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);

    let input = write_hook_in_dir(
        temp.path(),
        "packages/hurry/src/daemon.rs",
        "pub fn sync_again() {}\n",
    );
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);

    let input = user_prompt_hook_in_dir(temp.path(), "try running it");
    let (exit_code, output) = run_hook_in_project(&temp, &input);
    assert_expected(exit_code, &output, Expected::Continue);
}

#[test]
fn test_command_can_simulate_changed_file_for_semantic_prompt_rule() {
    let temp = project_with_semantic_prompt_rule();

    let output = Command::new(nudge_binary())
        .args([
            "test",
            "--rule",
            "hurry-local-test-reminder",
            "--prompt",
            "try executing it",
            "--changed-file",
            "packages/hurry/src/daemon.rs",
        ])
        .current_dir(temp.path())
        .env("HOME", temp.path().join("home"))
        .env("XDG_CONFIG_HOME", temp.path().join("xdg-config"))
        .env("NUDGE_STATE_DIR", temp.path().join("state"))
        .output()
        .expect("run nudge test");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected nudge test to pass, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(stdout.contains("Result: Continue"), "stdout: {stdout}");
    assert!(
        stdout.contains("Use `hurry-dev` after `make install-dev`."),
        "stdout: {stdout}"
    );
}

fn project_with_semantic_prompt_rule() -> TempDir {
    let temp = TempDir::new().expect("create temp dir");
    fs::create_dir_all(temp.path().join("packages/hurry/src")).expect("create source dir");
    fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: hurry-local-test-reminder
    description: Use dev entrypoints when testing hurry changes
    message: "Use `hurry-dev` after `make install-dev`."
    on:
      - hook: UserPromptSubmit
        intent:
          examples:
            - "let's test this"
            - "try running it"
            - "does this work"
        after_file_change:
          - file: "packages/hurry/src/**"
            within: "1h"
        once_per_change: true
        cooldown: "1h"
"#,
    )
    .expect("write config");
    temp
}

fn user_prompt_hook_in_dir(cwd: &std::path::Path, prompt: &str) -> String {
    serde_json::json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": cwd,
        "prompt": prompt
    })
    .to_string()
}

fn write_hook_in_dir(cwd: &std::path::Path, file_path: &str, content: &str) -> String {
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

fn run_hook_in_project(dir: &TempDir, input: &str) -> (i32, String) {
    let home = dir.path().join("home");
    let state = dir.path().join("state");
    let mut child = Command::new(nudge_binary())
        .args(["claude", "hook"])
        .current_dir(dir.path())
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", dir.path().join("xdg-config"))
        .env("NUDGE_STATE_DIR", &state)
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
