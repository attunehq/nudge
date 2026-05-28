//! Hook setup integration tests.

use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use serde_json::Value;
use tempfile::TempDir;

fn run_nudge(args: &[&str]) -> (i32, String, String) {
    let mut cmd_args = vec!["run", "--quiet", "-p", "nudge", "--"];
    cmd_args.extend(args);
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .expect("manifest dir has parent")
        .parent()
        .expect("parent has parent");

    let output = Command::new("cargo")
        .args(&cmd_args)
        .current_dir(workspace_root)
        .env("RUST_BACKTRACE", "0")
        .env("NUDGE_LOG", "error")
        .output()
        .expect("run nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}

fn run_built_nudge_in(dir: &TempDir, args: &[&str]) -> (i32, String, String) {
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "nudge"])
        .status()
        .expect("build nudge");
    assert!(status.success(), "cargo build failed");

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let binary = manifest_dir
        .parent()
        .expect("manifest dir has parent")
        .parent()
        .expect("parent has parent")
        .join("target/debug/nudge");

    let output = Command::new(binary)
        .args(args)
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run nudge")
        .wait_with_output()
        .expect("wait for nudge");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn claude_setup_is_idempotent_and_installs_only_handled_events() {
    let temp = TempDir::new().expect("temp dir");
    let claude_dir = temp.path().join(".claude");
    let claude_dir = claude_dir.to_str().expect("utf-8 path");

    let args = [
        "claude",
        "setup",
        "--claude-dir",
        claude_dir,
        "--skip-claude-md",
    ];
    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    let first = std::fs::read_to_string(temp.path().join(".claude/settings.local.json"))
        .expect("read settings");

    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    let second = std::fs::read_to_string(temp.path().join(".claude/settings.local.json"))
        .expect("read settings");

    pretty_assert_eq!(first, second);

    let json = serde_json::from_str::<Value>(&second).expect("valid json");
    assert!(json["hooks"]["PreToolUse"].is_array());
    assert!(json["hooks"]["UserPromptSubmit"].is_array());
    assert!(json["hooks"].get("PostToolUse").is_none());
    assert!(json["hooks"].get("Stop").is_none());
    assert_eq!(
        json["hooks"]["PreToolUse"][0]["matcher"],
        "Write|Edit|WebFetch|Bash"
    );

    let command = json["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .expect("command")
        .to_string();
    let mut with_old_events = json;
    with_old_events["hooks"]["PostToolUse"] = serde_json::json!([
        {
            "matcher": "*",
            "hooks": [{ "type": "command", "command": command, "timeout": 5 }]
        }
    ]);
    with_old_events["hooks"]["Stop"] = serde_json::json!([
        {
            "hooks": [{ "type": "command", "command": command, "timeout": 5 }]
        }
    ]);
    std::fs::write(
        temp.path().join(".claude/settings.local.json"),
        serde_json::to_string_pretty(&with_old_events).expect("serialize settings"),
    )
    .expect("write settings");

    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    let cleaned = serde_json::from_str::<Value>(
        &std::fs::read_to_string(temp.path().join(".claude/settings.local.json"))
            .expect("read settings"),
    )
    .expect("valid json");
    assert!(cleaned["hooks"].get("PostToolUse").is_none());
    assert!(cleaned["hooks"].get("Stop").is_none());
}

#[test]
fn codex_setup_creates_hooks_json_and_is_idempotent() {
    let temp = TempDir::new().expect("temp dir");
    let codex_dir = temp.path().join(".codex");
    let codex_dir = codex_dir.to_str().expect("utf-8 path");
    let args = ["codex", "setup", "--codex-dir", codex_dir];

    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    let first = std::fs::read_to_string(temp.path().join(".codex/hooks.json")).expect("read hooks");

    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    let second =
        std::fs::read_to_string(temp.path().join(".codex/hooks.json")).expect("read hooks");

    pretty_assert_eq!(first, second);

    let json = serde_json::from_str::<Value>(&second).expect("valid json");
    assert_eq!(
        json["hooks"]["PreToolUse"][0]["matcher"],
        "Bash|apply_patch"
    );
    assert!(json["hooks"]["UserPromptSubmit"][0]["hooks"].is_array());
}

#[test]
fn codex_setup_preserves_existing_unrelated_hooks() {
    let temp = TempDir::new().expect("temp dir");
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create .codex");
    std::fs::write(
        codex_dir.join("hooks.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
    )
    .expect("write hooks");

    let args = [
        "codex",
        "setup",
        "--codex-dir",
        codex_dir.to_str().expect("utf-8 path"),
    ];
    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");

    let json = serde_json::from_str::<Value>(
        &std::fs::read_to_string(codex_dir.join("hooks.json")).expect("read hooks"),
    )
    .expect("valid json");
    assert_eq!(
        json["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "echo hi"
    );
    assert!(json["hooks"]["PreToolUse"].is_array());
}

#[test]
fn codex_setup_warns_and_skips_inline_toml_hooks() {
    let temp = TempDir::new().expect("temp dir");
    let codex_dir = temp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create .codex");
    std::fs::write(codex_dir.join("config.toml"), "[hooks]\n").expect("write config");

    let args = [
        "codex",
        "setup",
        "--codex-dir",
        codex_dir.to_str().expect("utf-8 path"),
    ];
    let (exit_code, _stdout, stderr) = run_nudge(&args);

    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        stderr.contains("warning:"),
        "expected warning for inline hooks, got: {stderr}"
    );
    assert!(
        !codex_dir.join("hooks.json").exists(),
        "setup should skip hooks.json when inline hooks exist"
    );
}

#[test]
fn validate_warns_for_codex_unsupported_webfetch_rules() {
    let temp = TempDir::new().expect("temp dir");
    std::fs::create_dir_all(temp.path().join(".codex")).expect("create .codex");
    std::fs::write(
        temp.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: prefer-local-docs
    description: Read local docs
    message: "Use local docs"
    on:
      - hook: PreToolUse
        tool: WebFetch
        url:
          - kind: Regex
            pattern: "docs\\.rs"
"#,
    )
    .expect("write rules");

    let (exit_code, _stdout, stderr) = run_built_nudge_in(&temp, &["validate"]);

    pretty_assert_eq!(exit_code, 0, "validate failed: {stderr}");
    assert!(
        stderr.contains(
            "warning: rule \"prefer-local-docs\" uses PreToolUse WebFetch, which Claude Code supports but Codex hooks do not currently intercept."
        ),
        "expected Codex WebFetch warning, got: {stderr}"
    );
}
