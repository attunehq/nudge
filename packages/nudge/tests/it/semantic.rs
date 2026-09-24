use std::{fs, io::Write, process::Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use serde_json::json;
use tempfile::TempDir;

use crate::isolated_command;

const RULE: &str = r#"
version: 1
rules:
  - name: comments-explain-intent
    action: warn
    message: Explain the non-obvious intent or remove this comment.
    on:
      - hook: PreToolUse
        tool: Write
        file: '**/*.rs'
        semantic:
          select: {kind: Comments, language: rust}
          violates: The comment merely restates the adjacent code.
          allow: The comment explains intent or non-obvious context.
          thresholds: {clear: 0.1, violation: 0.9}
"#;

#[test]
fn validate_stays_offline_and_check_reports_missing_credentials() {
    let dir = TempDir::new().expect("temp repo");
    fs::write(dir.path().join(".nudge.yaml"), RULE).expect("rule");
    fs::write(
        dir.path().join("src.rs"),
        "fn ada() {\n// Call run\nrun();\n}\n",
    )
    .expect("source");
    for (args, expected) in [
        (vec!["validate", ".nudge.yaml"], 0),
        (vec!["check", "src.rs"], 2),
    ] {
        let output = isolated_command(dir.path())
            .args(args)
            .current_dir(dir.path())
            .env_remove("TYPESAFE_API_KEY")
            .output()
            .expect("run");
        pretty_assert_eq!(output.status.code(), Some(expected));
        if expected == 2 {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("Jev credential is missing"));
            assert!(!stdout.contains("✓ Checked"));
        }
    }
    fs::write(dir.path().join("src.rs"), "fn ada() {}\n").expect("source without comments");
    let output = isolated_command(dir.path())
        .args(["check", "src.rs"])
        .current_dir(dir.path())
        .env_remove("TYPESAFE_API_KEY")
        .output()
        .expect("run");
    pretty_assert_eq!(output.status.code(), Some(0));
}

#[test]
fn provider_hooks_deliver_incomplete_warning_without_blocking() {
    let dir = TempDir::new().expect("temp repo");
    fs::write(dir.path().join(".nudge.yaml"), RULE).expect("rule");
    for provider in ["claude", "codex", "grok", "cursor"] {
        let (tool, input) = if provider == "codex" {
            (
                "apply_patch",
                json!({"command":"*** Begin Patch\n*** Add File: src.rs\n+fn ada() {\n+// Call run\n+run();\n+}\n*** End Patch"}),
            )
        } else {
            (
                "Write",
                json!({"file_path":"src.rs","content":"fn ada() {\n// Call run\nrun();\n}"}),
            )
        };
        let payload = json!({"hook_event_name":"PreToolUse","tool_name":tool,"cwd":dir.path(),"tool_input":input});
        let mut child = isolated_command(dir.path())
            .args([provider, "hook"])
            .current_dir(dir.path())
            .env_remove("TYPESAFE_API_KEY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("hook");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.to_string().as_bytes())
            .expect("send hook");
        let output = child.wait_with_output().expect("hook response");
        pretty_assert_eq!(output.status.code(), Some(0));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Jev credential is missing"),
            "{provider}: {stdout}"
        );
        assert!(!stdout.contains("\"deny\""));
    }
}
