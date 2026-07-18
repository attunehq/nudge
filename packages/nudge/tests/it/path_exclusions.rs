//! Integration tests for ordered file include and exclusion globs.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

use crate::{edit_hook, nudge_binary, write_hook};

const CONFIG: &str = r#"
version: 1
rules:
  - name: no-any
    message: "Remove the explicit any."
    on:
      - hook: PreToolUse
        tool: Write
        file:
          - "**/*.ts"
          - "!**/*.gen.ts"
        content:
          - kind: Regex
            pattern: "as any"
      - hook: PreToolUse
        tool: Edit
        file:
          - "**/*.ts"
          - "!**/*.gen.ts"
        new_content:
          - kind: Regex
            pattern: "as any"
"#;

#[test]
fn hooks_skip_excluded_write_and_edit_paths() {
    let dir = configured_project();

    for input in [
        write_hook("src/routeTree.gen.ts", "value as any"),
        edit_hook("src/routeTree.gen.ts", "value", "value as any"),
    ] {
        let (exit_code, output) = run_nudge(&dir, &["claude", "hook"], Some(&input));
        pretty_assert_eq!(exit_code, 0, "excluded hook should pass: {output}");
        assert!(
            output.is_empty(),
            "excluded hook should be silent: {output}"
        );
    }

    for input in [
        write_hook("src/main.ts", "value as any"),
        edit_hook("src/main.ts", "value", "value as any"),
    ] {
        let (exit_code, output) = run_nudge(&dir, &["claude", "hook"], Some(&input));
        pretty_assert_eq!(
            exit_code,
            0,
            "matching hook should return a decision: {output}"
        );
        assert!(
            output.contains(r#""permissionDecision":"deny""#),
            "matching hook should interrupt: {output}"
        );
    }
}

#[test]
fn check_skips_excluded_files_per_rule() {
    let dir = configured_project();
    fs::create_dir_all(dir.path().join("src")).expect("create source directory");
    fs::write(dir.path().join("src/main.ts"), "value as any\n").expect("write checked file");
    fs::write(
        dir.path().join("src/routeTree.gen.ts"),
        "generated as any\n",
    )
    .expect("write excluded file");

    let (exit_code, output) = run_nudge(&dir, &["check"], None);

    pretty_assert_eq!(
        exit_code,
        1,
        "included violation should fail check: {output}"
    );
    assert!(output.contains("src\\main.ts") || output.contains("src/main.ts"));
    assert!(
        !output.contains("routeTree.gen.ts"),
        "excluded file was checked: {output}"
    );
}

fn configured_project() -> TempDir {
    let dir = TempDir::new().expect("create temp directory");
    fs::write(dir.path().join(".nudge.yaml"), CONFIG).expect("write Nudge config");
    dir
}

fn run_nudge(dir: &TempDir, args: &[&str], stdin: Option<&str>) -> (i32, String) {
    let mut child = Command::new(nudge_binary())
        .args(args)
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run nudge");

    if let Some(stdin) = stdin {
        child
            .stdin
            .as_mut()
            .expect("open stdin")
            .write_all(stdin.as_bytes())
            .expect("write stdin");
    }

    let output = child.wait_with_output().expect("wait for nudge");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), combined)
}
