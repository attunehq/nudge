//! Hook setup integration tests.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::nudge_binary;
use pretty_assertions::assert_eq as pretty_assert_eq;
use serde_json::{Value, json};
use tempfile::TempDir;

fn run_nudge(args: &[&str]) -> (i32, String, String) {
    run_nudge_binary(&nudge_binary(), args)
}

fn run_nudge_binary(binary: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(binary)
        .args(args)
        .env("RUST_BACKTRACE", "0")
        .env("NUDGE_LOG", "error")
        .output()
        .expect("run nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}

fn copy_nudge_binary_to_path_with_spaces(temp: &TempDir) -> PathBuf {
    let bin_dir = temp.path().join("bin with spaces");
    fs::create_dir_all(&bin_dir).expect("create spaced bin dir");

    let source = nudge_binary();
    let target = bin_dir.join(source.file_name().expect("binary filename"));
    fs::copy(&source, &target).expect("copy nudge binary");

    #[cfg(unix)]
    {
        let permissions = fs::metadata(&source)
            .expect("source metadata")
            .permissions();
        fs::set_permissions(&target, permissions).expect("preserve executable permissions");
    }

    target
}

#[test]
fn claude_setup_help_mentions_settings_local_json() {
    let (exit_code, stdout, stderr) = run_nudge(&["claude", "setup", "--help"]);

    pretty_assert_eq!(exit_code, 0, "help failed: {stderr}");
    assert!(
        stdout.contains(".claude/settings.local.json"),
        "help should mention settings.local.json, got: {stdout}"
    );
    assert!(
        !stdout.contains(".claude/hooks"),
        "help should not mention stale .claude/hooks path, got: {stdout}"
    );
    assert!(
        stdout.contains("--skip-skills"),
        "help should mention skill install opt-out, got: {stdout}"
    );
    assert!(
        stdout.contains("--skip-commands"),
        "help should mention slash command install opt-out, got: {stdout}"
    );
    assert!(
        !stdout.contains("--skip-claude-md"),
        "help should not mention removed CLAUDE.md mutation opt-out, got: {stdout}"
    );
}

#[test]
fn codex_setup_help_does_not_mention_command_install() {
    let (exit_code, stdout, stderr) = run_nudge(&["codex", "setup", "--help"]);

    pretty_assert_eq!(exit_code, 0, "help failed: {stderr}");
    assert!(
        stdout.contains("--skip-skills"),
        "help should mention skill install opt-out, got: {stdout}"
    );
    assert!(
        !stdout.contains("--skip-commands"),
        "Codex setup should not advertise unsupported project prompt commands, got: {stdout}"
    );
}

fn run_built_nudge_in(dir: &TempDir, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(nudge_binary())
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

fn assert_nudge_skill_installed(skill_dir: &Path) {
    let skill = fs::read_to_string(skill_dir.join("SKILL.md")).expect("read SKILL.md");
    let ci = fs::read_to_string(skill_dir.join("references/ci.md")).expect("read ci");
    let hook_responses =
        fs::read_to_string(skill_dir.join("references/hook-responses.md")).expect("read hooks");
    let rule_debugging = fs::read_to_string(skill_dir.join("references/rule-debugging.md"))
        .expect("read rule debugging");
    let rule_writing =
        fs::read_to_string(skill_dir.join("references/rule-writing.md")).expect("read rules");
    let validation =
        fs::read_to_string(skill_dir.join("references/validation.md")).expect("read validation");

    assert!(skill.contains("name: nudge"));
    assert!(skill.contains("nudge-learnings"));
    assert!(skill.contains("references/ci.md"));
    assert!(skill.contains("references/hook-responses.md"));
    assert!(skill.contains("references/rule-debugging.md"));
    assert!(skill.contains("references/rule-writing.md"));
    assert!(skill.contains("references/validation.md"));
    assert!(ci.contains("Nudge CI"));
    assert!(hook_responses.contains("Nudge Hook Responses"));
    assert!(rule_debugging.contains("Nudge Rule Debugging"));
    assert!(rule_writing.contains("Nudge Rule Writing"));
    assert!(validation.contains("Nudge Validation"));
}

fn assert_nudge_learnings_skill_installed(skill_dir: &Path) {
    let skill = fs::read_to_string(skill_dir.join("SKILL.md")).expect("read learnings SKILL.md");
    let learnings =
        fs::read_to_string(skill_dir.join("references/learnings.md")).expect("read learnings");
    let learnings_bm25 = fs::read_to_string(skill_dir.join("references/learnings-bm25.md"))
        .expect("read learnings bm25");
    let learnings_embeddings =
        fs::read_to_string(skill_dir.join("references/learnings-embeddings.md"))
            .expect("read learnings embeddings");

    assert!(skill.contains("name: nudge-learnings"));
    assert!(skill.contains("search before deep investigation"));
    assert!(skill.contains("nudge learn add"));
    assert!(learnings.contains("Nudge Learnings"));
    assert!(learnings_bm25.contains("Nudge Learnings Without Embeddings"));
    assert!(learnings_embeddings.contains("Nudge Learnings With Local Embeddings"));
}

fn assert_claude_nudge_learn_command_installed(project_root: &Path) {
    let command_path = project_root.join(".claude/commands/nudge/learn.md");
    let command = fs::read_to_string(&command_path).expect("read Claude nudge learn command");

    assert!(command.contains("Review the current session history"));
    assert!(command.contains("nudge learn add"));
    assert!(command.contains("## What went wrong"));
    assert!(command.contains("## Verification"));
    assert!(command.contains("$ARGUMENTS"));
}

#[test]
fn claude_setup_does_not_modify_claude_md() {
    let temp = TempDir::new().expect("temp dir");
    let claude_md = temp.path().join("CLAUDE.md");
    let original = "# CLAUDE.md\n\nExisting project guidance.\n";
    fs::write(&claude_md, original).expect("write CLAUDE.md");

    let (exit_code, stdout, stderr) = run_built_nudge_in(&temp, &["claude", "setup"]);

    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        !stdout.contains("Add this section to CLAUDE.md"),
        "setup should not prompt to edit CLAUDE.md, got: {stdout}"
    );
    pretty_assert_eq!(
        fs::read_to_string(&claude_md).expect("read CLAUDE.md"),
        original
    );
    assert_nudge_skill_installed(&temp.path().join(".claude/skills/nudge"));
    assert_nudge_learnings_skill_installed(&temp.path().join(".claude/skills/nudge-learnings"));
    assert_claude_nudge_learn_command_installed(temp.path());
}

#[test]
fn claude_setup_is_idempotent_and_installs_only_handled_events() {
    let temp = TempDir::new().expect("temp dir");
    let claude_dir = temp.path().join(".claude");
    let claude_dir = claude_dir.to_str().expect("utf-8 path");

    let args = ["claude", "setup", "--claude-dir", claude_dir];
    let (exit_code, stdout, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        !stdout.contains("Backed up previous configuration"),
        "fresh setup should not report a backup, got: {stdout}"
    );
    assert!(
        !temp.path().join(".claude/settings.local.json.bak").exists(),
        "fresh setup should not create a backup"
    );
    assert!(
        stdout.contains("Installed nudge skill"),
        "fresh setup should install bundled router skill, got: {stdout}"
    );
    assert!(
        stdout.contains("Installed nudge-learnings skill"),
        "fresh setup should install bundled learnings skill, got: {stdout}"
    );
    assert!(
        stdout.contains("Installed nudge:learn command"),
        "fresh setup should install bundled slash command, got: {stdout}"
    );
    assert_nudge_skill_installed(&temp.path().join(".claude/skills/nudge"));
    assert_nudge_learnings_skill_installed(&temp.path().join(".claude/skills/nudge-learnings"));
    assert_claude_nudge_learn_command_installed(temp.path());
    assert!(
        !temp.path().join("CLAUDE.md").exists(),
        "fresh setup should not create CLAUDE.md"
    );
    let first =
        fs::read_to_string(temp.path().join(".claude/settings.local.json")).expect("read settings");

    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    let second =
        fs::read_to_string(temp.path().join(".claude/settings.local.json")).expect("read settings");

    pretty_assert_eq!(first, second);

    let json = serde_json::from_str::<Value>(&second).expect("valid json");
    assert!(json["hooks"]["PreToolUse"].is_array());
    assert!(json["hooks"]["UserPromptSubmit"].is_array());
    assert!(json["hooks"].get("PostToolUse").is_none());
    assert!(json["hooks"].get("Stop").is_none());
    pretty_assert_eq!(
        json["hooks"]["PreToolUse"][0]["matcher"],
        "Write|Edit|WebFetch|Bash"
    );

    let command = json["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .expect("command")
        .to_string();
    let mut with_old_events = json;
    with_old_events["hooks"]["PostToolUse"] = json!([
        {
            "matcher": "*",
            "hooks": [{ "type": "command", "command": command, "timeout": 5 }]
        }
    ]);
    with_old_events["hooks"]["Stop"] = json!([
        {
            "hooks": [{ "type": "command", "command": command, "timeout": 5 }]
        }
    ]);
    fs::write(
        temp.path().join(".claude/settings.local.json"),
        serde_json::to_string_pretty(&with_old_events).expect("serialize settings"),
    )
    .expect("write settings");

    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    let cleaned = serde_json::from_str::<Value>(
        &fs::read_to_string(temp.path().join(".claude/settings.local.json"))
            .expect("read settings"),
    )
    .expect("valid json");
    assert!(cleaned["hooks"].get("PostToolUse").is_none());
    assert!(cleaned["hooks"].get("Stop").is_none());
}

#[test]
fn claude_setup_can_skip_bundled_commands() {
    let temp = TempDir::new().expect("temp dir");
    let claude_dir = temp.path().join(".claude");
    let args = [
        "claude",
        "setup",
        "--claude-dir",
        claude_dir.to_str().expect("utf-8 path"),
        "--skip-commands",
    ];

    let (exit_code, stdout, stderr) = run_nudge(&args);

    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        !stdout.contains("Installed nudge:learn command"),
        "setup should not report command install when skipped, got: {stdout}"
    );
    assert!(
        !claude_dir.join("commands/nudge/learn.md").exists(),
        "setup should not install bundled slash command when skipped"
    );
}

#[test]
fn claude_setup_backs_up_existing_settings_without_overwriting_backups() {
    let temp = TempDir::new().expect("temp dir");
    let claude_dir = temp.path().join(".claude");
    fs::create_dir_all(&claude_dir).expect("create .claude");
    let canonical_claude_dir = claude_dir.canonicalize().expect("canonical .claude");

    let settings_file = claude_dir.join("settings.local.json");
    let first_backup = canonical_claude_dir.join("settings.local.json.bak");
    let second_backup = canonical_claude_dir.join("settings.local.json.bak.1");
    let original = r#"{"env":{"KEEP":"yes"}}"#;
    fs::write(&settings_file, original).expect("write settings");

    let args = [
        "claude",
        "setup",
        "--claude-dir",
        claude_dir.to_str().expect("utf-8 path"),
    ];
    let (exit_code, stdout, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        stdout.contains(&format!(
            "Backed up previous configuration to {}",
            first_backup.display()
        )),
        "setup should print backup path, got: {stdout}"
    );
    pretty_assert_eq!(
        fs::read_to_string(&first_backup).expect("read backup"),
        original
    );
    let installed = fs::read_to_string(&settings_file).expect("read installed settings");

    let (exit_code, stdout, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        stdout.contains(&format!(
            "Backed up previous configuration to {}",
            second_backup.display()
        )),
        "repeated setup should print next backup path, got: {stdout}"
    );
    pretty_assert_eq!(
        fs::read_to_string(&first_backup).expect("read first backup"),
        original,
        "repeated setup must not overwrite the first backup"
    );
    pretty_assert_eq!(
        fs::read_to_string(second_backup).expect("read second backup"),
        installed
    );
}

#[test]
fn claude_setup_quotes_binary_path_with_spaces() {
    let temp = TempDir::new().expect("temp dir");
    let binary = copy_nudge_binary_to_path_with_spaces(&temp);
    let claude_dir = temp.path().join(".claude");
    let args = [
        "claude",
        "setup",
        "--claude-dir",
        claude_dir.to_str().expect("utf-8 path"),
    ];

    let (exit_code, _stdout, stderr) = run_nudge_binary(&binary, &args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");

    let json = serde_json::from_str::<Value>(
        &fs::read_to_string(claude_dir.join("settings.local.json")).expect("read settings"),
    )
    .expect("valid json");
    let command = json["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .expect("command");
    assert!(
        command.starts_with('\''),
        "expected shell-quoted command for spaced path, got: {command}"
    );
    let words = shell_words::split(command).expect("split command");
    pretty_assert_eq!(
        words,
        vec![
            binary.to_str().expect("utf-8 binary").to_string(),
            "claude".to_string(),
            "hook".to_string()
        ]
    );
}

#[test]
fn codex_setup_creates_hooks_json_and_is_idempotent() {
    let temp = TempDir::new().expect("temp dir");
    let codex_dir = temp.path().join(".codex");
    let codex_dir = codex_dir.to_str().expect("utf-8 path");
    let args = ["codex", "setup", "--codex-dir", codex_dir];

    let (exit_code, stdout, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        !stdout.contains("Backed up previous configuration"),
        "fresh setup should not report a backup, got: {stdout}"
    );
    assert!(
        !temp.path().join(".codex/hooks.json.bak").exists(),
        "fresh setup should not create a backup"
    );
    assert!(
        stdout.contains("Installed nudge skill"),
        "fresh setup should install bundled router skill, got: {stdout}"
    );
    assert!(
        stdout.contains("Installed nudge-learnings skill"),
        "fresh setup should install bundled learnings skill, got: {stdout}"
    );
    assert!(
        !stdout.contains("prompt command"),
        "Codex setup should not report unsupported prompt command install, got: {stdout}"
    );
    assert_nudge_skill_installed(&temp.path().join(".agents/skills/nudge"));
    assert_nudge_learnings_skill_installed(&temp.path().join(".agents/skills/nudge-learnings"));
    assert!(
        !temp.path().join(".codex/prompts/nudge-learn.md").exists(),
        "Codex setup should not install unsupported project prompt commands"
    );
    let first = fs::read_to_string(temp.path().join(".codex/hooks.json")).expect("read hooks");

    let (exit_code, _, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    let second = fs::read_to_string(temp.path().join(".codex/hooks.json")).expect("read hooks");

    pretty_assert_eq!(first, second);

    let json = serde_json::from_str::<Value>(&second).expect("valid json");
    pretty_assert_eq!(
        json["hooks"]["PreToolUse"][0]["matcher"],
        "Bash|apply_patch"
    );
    assert!(json["hooks"]["UserPromptSubmit"][0]["hooks"].is_array());
}

#[test]
fn codex_setup_backs_up_existing_hooks_without_overwriting_backups() {
    let temp = TempDir::new().expect("temp dir");
    let codex_dir = temp.path().join(".codex");
    fs::create_dir_all(&codex_dir).expect("create .codex");
    let canonical_codex_dir = codex_dir.canonicalize().expect("canonical .codex");

    let hooks_file = codex_dir.join("hooks.json");
    let first_backup = canonical_codex_dir.join("hooks.json.bak");
    let second_backup = canonical_codex_dir.join("hooks.json.bak.1");
    let original =
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#;
    fs::write(&hooks_file, original).expect("write hooks");

    let args = [
        "codex",
        "setup",
        "--codex-dir",
        codex_dir.to_str().expect("utf-8 path"),
    ];
    let (exit_code, stdout, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        stdout.contains(&format!(
            "Backed up previous configuration to {}",
            first_backup.display()
        )),
        "setup should print backup path, got: {stdout}"
    );
    pretty_assert_eq!(
        fs::read_to_string(&first_backup).expect("read backup"),
        original
    );
    let installed = fs::read_to_string(&hooks_file).expect("read installed hooks");

    let (exit_code, stdout, stderr) = run_nudge(&args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");
    assert!(
        stdout.contains(&format!(
            "Backed up previous configuration to {}",
            second_backup.display()
        )),
        "repeated setup should print next backup path, got: {stdout}"
    );
    pretty_assert_eq!(
        fs::read_to_string(&first_backup).expect("read first backup"),
        original,
        "repeated setup must not overwrite the first backup"
    );
    pretty_assert_eq!(
        fs::read_to_string(second_backup).expect("read second backup"),
        installed
    );
}

#[test]
fn codex_setup_quotes_binary_path_with_spaces() {
    let temp = TempDir::new().expect("temp dir");
    let binary = copy_nudge_binary_to_path_with_spaces(&temp);
    let codex_dir = temp.path().join(".codex");
    let args = [
        "codex",
        "setup",
        "--codex-dir",
        codex_dir.to_str().expect("utf-8 path"),
    ];

    let (exit_code, _stdout, stderr) = run_nudge_binary(&binary, &args);
    pretty_assert_eq!(exit_code, 0, "setup failed: {stderr}");

    let json = serde_json::from_str::<Value>(
        &fs::read_to_string(codex_dir.join("hooks.json")).expect("read hooks"),
    )
    .expect("valid json");
    let command = json["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .expect("command");
    assert!(
        command.starts_with('\''),
        "expected shell-quoted command for spaced path, got: {command}"
    );
    let words = shell_words::split(command).expect("split command");
    pretty_assert_eq!(
        words,
        vec![
            binary.to_str().expect("utf-8 binary").to_string(),
            "codex".to_string(),
            "hook".to_string()
        ]
    );
}

#[test]
fn codex_setup_preserves_existing_unrelated_hooks() {
    let temp = TempDir::new().expect("temp dir");
    let codex_dir = temp.path().join(".codex");
    fs::create_dir_all(&codex_dir).expect("create .codex");
    fs::write(
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
        &fs::read_to_string(codex_dir.join("hooks.json")).expect("read hooks"),
    )
    .expect("valid json");
    pretty_assert_eq!(
        json["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "echo hi"
    );
    assert!(json["hooks"]["PreToolUse"].is_array());
}

#[test]
fn codex_setup_warns_and_skips_inline_toml_hooks() {
    let temp = TempDir::new().expect("temp dir");
    let codex_dir = temp.path().join(".codex");
    fs::create_dir_all(&codex_dir).expect("create .codex");
    fs::write(codex_dir.join("config.toml"), "[hooks]\n").expect("write config");

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
    assert_nudge_skill_installed(&temp.path().join(".agents/skills/nudge"));
    assert_nudge_learnings_skill_installed(&temp.path().join(".agents/skills/nudge-learnings"));
    assert!(
        !codex_dir.join("prompts/nudge-learn.md").exists(),
        "Codex setup should not install unsupported project prompt commands"
    );
}

#[test]
fn validate_warns_for_codex_unsupported_webfetch_rules() {
    let temp = TempDir::new().expect("temp dir");
    fs::create_dir_all(temp.path().join(".codex")).expect("create .codex");
    fs::write(
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
