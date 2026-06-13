//! Integration tests for provider-free `nudge check` behavior.

use std::fs;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

use crate::nudge_binary;

fn run_nudge_in_dir(dir: &TempDir, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(nudge_binary())
        .args(args)
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}

fn yaml_command(command: &[String]) -> String {
    serde_json::to_string(command).expect("serialize command")
}

fn required_marker_command(marker: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        vec![String::from("findstr"), marker.to_string()]
    }

    #[cfg(not(windows))]
    {
        vec![String::from("grep"), String::from("-q"), marker.to_string()]
    }
}

fn contains_path(output: &str, path: &str) -> bool {
    output.contains(path) || output.contains(&path.replace('/', "\\"))
}

#[test]
fn check_runs_file_rules_without_agent_provider() {
    let dir = TempDir::new().expect("create temp dir");
    let approved_command = required_marker_command("APPROVED");
    let approved_command_yaml = yaml_command(&approved_command);
    let approved_command_display = shell_words::join(&approved_command);
    fs::write(
        dir.path().join(".nudge.yaml"),
        format!(
            r#"
version: 1
rules:
  - name: block-todo-write
    description: Block TODO markers in committed text files
    message: "Resolve TODO `{{{{ $todo }}}}` before commit."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: Regex
            pattern: "TODO: (?P<todo>.+)"
  - name: block-fixme-edit
    description: Block FIXME markers introduced by edits
    message: "Resolve FIXME before commit."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.txt"
        new_content:
          - kind: Regex
            pattern: "FIXME"
  - name: require-approved-marker
    description: Markdown release notes require an approval marker
    message: "Add APPROVED marker. Verify with `{{{{ $command }}}}`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.md"
        content:
          - kind: External
            command: {approved_command_yaml}
  - name: ignored-bash-substitute
    action: substitute
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install"
            replace: "yarn add"
  - name: ignored-webfetch
    message: "Hook-only WebFetch rule."
    on:
      - hook: PreToolUse
        tool: WebFetch
        url:
          - kind: Regex
            pattern: "example.com"
  - name: ignored-prompt
    message: "Hook-only prompt reminder."
    on:
      - hook: UserPromptSubmit
        prompt:
          - kind: Regex
            pattern: "release"
"#
        ),
    )
    .expect("write config");

    fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    fs::create_dir_all(dir.path().join("docs")).expect("create docs dir");
    fs::write(
        dir.path().join("src/bad.txt"),
        "TODO: remove temporary note\nFIXME: finish this\n",
    )
    .expect("write bad txt");
    fs::write(
        dir.path().join("src/good.txt"),
        "Plain text with no markers.\n",
    )
    .expect("write good txt");
    fs::write(
        dir.path().join("docs/bad.md"),
        "# Release Notes\n\nMissing the marker.\n",
    )
    .expect("write bad markdown");
    fs::write(
        dir.path().join("docs/good.md"),
        "# Release Notes\n\nAPPROVED\n",
    )
    .expect("write good markdown");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "src", "docs"]);

    pretty_assert_eq!(
        exit_code,
        1,
        "check should fail for file-rule violations, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        contains_path(&stdout, "src/bad.txt:1 [block-todo-write]"),
        "expected Write regex issue with line number, got: {stdout}"
    );
    assert!(
        stdout.contains("Resolve TODO `remove temporary note` before commit."),
        "expected regex capture interpolation, got: {stdout}"
    );
    assert!(
        contains_path(&stdout, "src/bad.txt:2 [block-fixme-edit]"),
        "expected Edit regex issue with line number, got: {stdout}"
    );
    assert!(
        contains_path(&stdout, "docs/bad.md:1 [require-approved-marker]"),
        "expected External issue with line number, got: {stdout}"
    );
    assert!(
        stdout.contains(&format!("Verify with `{approved_command_display}`.")),
        "expected External command interpolation, got: {stdout}"
    );
    assert!(
        !stdout.contains("ignored-bash-substitute")
            && !stdout.contains("ignored-webfetch")
            && !stdout.contains("ignored-prompt"),
        "hook-only rules should not report in check mode, got: {stdout}"
    );
    assert!(
        !stdout.contains("src/good.txt") && !stdout.contains("docs/good.md"),
        "passing files should not report issues, got: {stdout}"
    );
}

#[test]
fn check_fails_when_explicit_path_does_not_exist() {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(
        dir.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: block-todo
    message: "Resolve TODO before commit."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: Regex
            pattern: "TODO"
"#,
    )
    .expect("write config");

    let (exit_code, stdout, stderr) =
        run_nudge_in_dir(&dir, &["check", "definitely-does-not-exist.rs"]);

    pretty_assert_eq!(
        exit_code,
        1,
        "check should fail for missing explicit paths, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stderr.contains("check path does not exist: definitely-does-not-exist.rs"),
        "expected missing path error, got stderr: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "expected no success output for missing path, got: {stdout}"
    );
}

#[test]
fn check_fails_when_glob_pattern_matches_no_files() {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(
        dir.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: block-todo
    message: "Resolve TODO before commit."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.txt"
        content:
          - kind: Regex
            pattern: "TODO"
"#,
    )
    .expect("write config");
    fs::create_dir_all(dir.path().join("src")).expect("create src dir");
    fs::write(dir.path().join("src/main.txt"), "Plain text.\n").expect("write file");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "**/*.rs"]);

    pretty_assert_eq!(
        exit_code,
        1,
        "check should fail for zero-match glob patterns, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stderr.contains("check glob pattern matched no files: **/*.rs"),
        "expected zero-match glob error, got stderr: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "expected no success output for zero-match glob, got: {stdout}"
    );
}

#[test]
fn check_exits_successfully_when_no_file_rules_exist() {
    let dir = TempDir::new().expect("create temp dir");
    fs::write(
        dir.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: prompt-only
    message: "Remember the release checklist."
    on:
      - hook: UserPromptSubmit
        prompt:
          - kind: Regex
            pattern: "release"
"#,
    )
    .expect("write config");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check"]);

    pretty_assert_eq!(
        exit_code,
        0,
        "check should pass when nothing is checkable, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("No file-based rules found."),
        "expected no-file-rule summary, got: {stdout}"
    );
}
