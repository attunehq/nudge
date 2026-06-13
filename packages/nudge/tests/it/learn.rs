//! Learned repo knowledge tests.

use std::{
    fs,
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
};

use crate::nudge_binary;
use pretty_assertions::assert_eq as pretty_assert_eq;
use serde_json::json;
use tempfile::TempDir;

fn run_nudge_in(root: &Path, args: &[&str], stdin: Option<&str>) -> (i32, String, String) {
    let mut child = Command::new(nudge_binary())
        .args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn nudge");

    if let Some(stdin) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(stdin.as_bytes())
            .expect("write stdin");
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait for nudge");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn learn_add_list_and_search_round_trip() {
    let temp = TempDir::new().expect("temp dir");

    let (exit_code, stdout, stderr) = run_nudge_in(
        temp.path(),
        &[
            "learn",
            "add",
            "--title",
            "Expo Metro resolver cache",
            "--body",
            "What went wrong: Expo could not resolve modules after a dependency update.\n\nFix: clear the Metro cache and restart the dev server.",
        ],
        None,
    );
    pretty_assert_eq!(exit_code, 0, "add failed: {stderr}");
    assert!(
        stdout.contains(".nudge") && stdout.contains("expo-metro-resolver-cache.md"),
        "add should report created note path, got: {stdout}"
    );
    assert!(
        temp.path()
            .join(".nudge/learned/expo-metro-resolver-cache.md")
            .exists(),
        "learn add should create the note"
    );

    let (exit_code, stdout, stderr) = run_nudge_in(temp.path(), &["learn", "list"], None);
    pretty_assert_eq!(exit_code, 0, "list failed: {stderr}");
    assert!(
        stdout.contains("Expo Metro resolver cache"),
        "list should include note title, got: {stdout}"
    );

    let (exit_code, stdout, stderr) = run_nudge_in(
        temp.path(),
        &[
            "learn", "search", "expo", "cannot", "resolve", "module", "metro",
        ],
        None,
    );
    pretty_assert_eq!(exit_code, 0, "search failed: {stderr}");
    assert!(
        stdout.contains("Expo Metro resolver cache"),
        "search should find the learned incident, got: {stdout}"
    );
    assert!(
        stdout.contains("clear the Metro cache"),
        "search should show a useful excerpt, got: {stdout}"
    );
}

#[test]
fn user_prompt_hook_injects_relevant_learned_context() {
    let temp = TempDir::new().expect("temp dir");
    let learned_dir = temp.path().join(".nudge/learned");
    fs::create_dir_all(&learned_dir).expect("create learned dir");
    fs::write(
        learned_dir.join("expo-metro-resolver-cache.md"),
        "# Expo Metro resolver cache\n\nWhat went wrong: Expo could not resolve modules after a dependency update.\n\nFix: clear the Metro cache and restart the dev server.\n\nVerification: expo start completed and the app loaded.",
    )
    .expect("write learned note");

    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "test",
        "transcript_path": temp.path().join("transcript.jsonl"),
        "permission_mode": "default",
        "cwd": temp.path(),
        "prompt": "Expo is failing again after a dependency update and Metro says it cannot resolve a module."
    })
    .to_string();

    let (exit_code, stdout, stderr) =
        run_nudge_in(temp.path(), &["claude", "hook"], Some(&payload));

    pretty_assert_eq!(exit_code, 0, "hook failed: {stderr}");
    assert!(
        stdout.contains("Nudge found learned repo knowledge"),
        "hook should inject learned context, got: {stdout}"
    );
    assert!(
        stdout.contains("Expo Metro resolver cache"),
        "hook context should mention the learned note, got: {stdout}"
    );
    assert!(
        stdout.contains("clear the Metro cache"),
        "hook context should include the fix excerpt, got: {stdout}"
    );
}

#[test]
fn learn_embeddings_enable_writes_project_config_without_reindex() {
    let temp = TempDir::new().expect("temp dir");

    let (exit_code, stdout, stderr) = run_nudge_in(
        temp.path(),
        &[
            "learn",
            "embeddings",
            "enable",
            "--model",
            "BAAI/bge-small-en-v1.5",
            "--no-reindex",
        ],
        None,
    );

    pretty_assert_eq!(exit_code, 0, "enable failed: {stderr}");
    assert!(
        stdout.contains("Enabled local learned-note embeddings"),
        "enable should report config update, got: {stdout}"
    );

    let config = fs::read_to_string(temp.path().join(".nudge.yaml")).expect("read config");
    assert!(
        config.contains("learn:") && config.contains("enabled: true"),
        "config should enable embeddings, got: {config}"
    );
    assert!(
        config.contains("model: BGESmallENV15"),
        "config should store the canonical model, got: {config}"
    );

    let (exit_code, stdout, stderr) =
        run_nudge_in(temp.path(), &["learn", "embeddings", "status"], None);

    pretty_assert_eq!(exit_code, 0, "status failed: {stderr}");
    assert!(
        stdout.contains("Embeddings: enabled") && stdout.contains("BGESmallENV15"),
        "status should report enabled model, got: {stdout}"
    );
}

#[test]
fn learn_embeddings_status_reads_nudge_yml() {
    let temp = TempDir::new().expect("temp dir");
    fs::write(
        temp.path().join(".nudge.yml"),
        r#"
version: 1
rules: []
learn:
  embeddings:
    enabled: true
    model: all-MiniLM-L6-v2
"#,
    )
    .expect("write config");

    let (exit_code, stdout, stderr) =
        run_nudge_in(temp.path(), &["learn", "embeddings", "status"], None);

    pretty_assert_eq!(exit_code, 0, "status failed: {stderr}");
    assert!(
        stdout.contains("Embeddings: enabled") && stdout.contains("all-MiniLM-L6-v2"),
        "status should read .nudge.yml learn config, got: {stdout}"
    );
}
