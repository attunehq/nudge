//! Integration tests for Bash tool matching with project_state.

use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

/// Create a temporary directory with a .nudge.yaml config containing the given
/// rules.
fn setup_config(rules_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join(".nudge.yaml");
    fs::write(&config_path, rules_yaml).expect("write config");
    dir
}

/// Create a temporary git repository with the given branch name.
fn setup_git_repo(branch_name: &str, rules_yaml: &str) -> TempDir {
    let dir = setup_config(rules_yaml);
    let temp_path = dir.path();

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(temp_path)
        .output()
        .expect("git init");

    // Configure git user for commits (required for some git operations)
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp_path)
        .output()
        .expect("git config email");

    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(temp_path)
        .output()
        .expect("git config name");

    // Create initial commit so we have a branch
    fs::write(temp_path.join("README.md"), "# Test").expect("write readme");
    Command::new("git")
        .args(["add", "."])
        .current_dir(temp_path)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp_path)
        .output()
        .expect("git commit");

    // Rename branch to desired name
    Command::new("git")
        .args(["branch", "-m", branch_name])
        .current_dir(temp_path)
        .output()
        .expect("git branch rename");

    dir
}

/// Get the path to the built nudge binary.
fn get_binary_path() -> PathBuf {
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "nudge"])
        .status()
        .expect("failed to build nudge");
    assert!(status.success(), "cargo build failed");

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    workspace_root.join("target/debug/nudge")
}

/// Run nudge claude hook with the given input JSON in the specified directory.
fn run_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let binary = get_binary_path();

    let mut child = Command::new(&binary)
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

/// Build a PreToolUse hook JSON payload for Bash tool.
fn bash_hook(command: &str, cwd: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": cwd,
        "tool_name": "Bash",
        "tool_use_id": "123",
        "tool_input": {
            "command": command,
            "description": "Test command"
        }
    })
    .to_string()
}

#[test]
fn test_bash_command_match() {
    let config = r#"
version: 1
rules:
  - name: block-rm-rf
    description: Block dangerous rm commands
    message: "Dangerous rm command detected"
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "rm\\s+-rf"
"#;

    let dir = setup_config(config);

    // Should match: rm -rf command
    let input = bash_hook("rm -rf /some/path", dir.path().to_str().unwrap());
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for rm -rf command, got: {output}"
    );
}

#[test]
fn test_bash_command_no_match() {
    let config = r#"
version: 1
rules:
  - name: block-rm-rf
    description: Block dangerous rm commands
    message: "Dangerous rm command detected"
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "rm\\s+-rf"
"#;

    let dir = setup_config(config);

    // Should not match: safe rm command
    let input = bash_hook("rm file.txt", dir.path().to_str().unwrap());
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.is_empty(),
        "expected passthrough for safe rm, got: {output}"
    );

    // Should not match: unrelated command
    let input = bash_hook("ls -la", dir.path().to_str().unwrap());
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.is_empty(),
        "expected passthrough for ls, got: {output}"
    );
}

#[test]
fn test_bash_project_state_git_branch_match() {
    let config = r#"
version: 1
rules:
  - name: block-main-push
    description: Block git push on main
    message: "git push is not allowed on main"
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "git\\s+push"
        project_state:
          - kind: Git
            branch:
              - kind: Regex
                pattern: "^main$"
"#;

    // Create git repo on main branch
    let dir = setup_git_repo("main", config);

    // Should match: git push on main branch
    let input = bash_hook("git push origin main", dir.path().to_str().unwrap());
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for git push on main, got: {output}"
    );
    assert!(
        output.contains("git push is not allowed"),
        "output should contain rule message: {output}"
    );
}

#[test]
fn test_bash_project_state_git_branch_no_match() {
    let config = r#"
version: 1
rules:
  - name: block-main-push
    description: Block git push on main
    message: "git push is not allowed on main"
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "git\\s+push"
        project_state:
          - kind: Git
            branch:
              - kind: Regex
                pattern: "^main$"
"#;

    // Create git repo on feature branch
    let dir = setup_git_repo("feature-branch", config);

    // Should NOT match: git push on feature branch (not main)
    let input = bash_hook(
        "git push origin feature-branch",
        dir.path().to_str().unwrap(),
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.is_empty(),
        "expected passthrough for git push on feature branch, got: {output}"
    );
}

#[test]
fn test_bash_project_state_non_git_dir() {
    let config = r#"
version: 1
rules:
  - name: block-main-push
    description: Block git push on main
    message: "git push is not allowed on main"
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "git\\s+push"
        project_state:
          - kind: Git
            branch:
              - kind: Regex
                pattern: "^main$"
"#;

    // Create a non-git directory (just use setup_config without git init)
    let dir = setup_config(config);

    // Should NOT match: not a git repo, so project_state fails (with warning)
    let input = bash_hook("git push origin main", dir.path().to_str().unwrap());
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.is_empty(),
        "expected passthrough for non-git directory, got: {output}"
    );
}

#[test]
fn test_bash_without_project_state() {
    let config = r#"
version: 1
rules:
  - name: block-all-push
    description: Block all git push commands
    message: "git push is blocked everywhere"
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "git\\s+push"
"#;

    let dir = setup_config(config);

    // Should match: git push without project_state requirement
    let input = bash_hook("git push origin main", dir.path().to_str().unwrap());
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for git push, got: {output}"
    );
}

#[test]
fn test_bash_multiple_branch_patterns() {
    let config = r#"
version: 1
rules:
  - name: block-main-push
    description: Block git push on main or master
    message: "git push is not allowed on main or master"
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "git\\s+push"
        project_state:
          - kind: Git
            branch:
              - kind: Regex
                pattern: "^(main|master)$"
"#;

    // Create git repo on main branch
    let dir = setup_git_repo("main", config);

    // Should match: git push on main branch
    let input = bash_hook("git push", dir.path().to_str().unwrap());
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for git push on main, got: {output}"
    );
}
