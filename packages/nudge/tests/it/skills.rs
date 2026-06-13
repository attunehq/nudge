//! Bundled skill installation tests.

use std::{fs, path::Path, process::Command};

use crate::{nudge_binary, run_nudge};
use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

fn run_nudge_in(root: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(nudge_binary())
        .args(args)
        .current_dir(root)
        .output()
        .expect("run nudge");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn learn_docs_prints_bundled_learnings_skill() {
    let (exit_code, stdout, stderr) = run_nudge(&["learn", "docs"]);

    pretty_assert_eq!(exit_code, 0, "learn docs failed: {stderr}");
    assert!(
        stdout.contains("# SKILL.md")
            && stdout.contains("name: nudge-learnings")
            && stdout.contains("references/bm25.md")
            && stdout.contains("references/embeddings.md"),
        "learn docs should print the bundled skill files, got: {stdout}"
    );
    assert!(
        stdout.contains("nudge learn embeddings status"),
        "learn docs should include retrieval-mode guidance, got: {stdout}"
    );
}

#[test]
fn claude_skills_install_writes_bundled_skills() {
    let temp = TempDir::new().expect("temp dir");

    let (exit_code, stdout, stderr) = run_nudge_in(temp.path(), &["claude", "skills", "install"]);

    pretty_assert_eq!(exit_code, 0, "skill install failed: {stderr}");
    assert!(
        stdout.contains("Installed nudge skill"),
        "install should report destination, got: {stdout}"
    );
    assert!(
        stdout.contains("Installed nudge-learnings skill"),
        "install should report destination, got: {stdout}"
    );

    let skill_dir = temp.path().join(".claude/skills/nudge");
    let skill = fs::read_to_string(skill_dir.join("SKILL.md")).expect("read SKILL.md");
    let hook_responses =
        fs::read_to_string(skill_dir.join("references/hook-responses.md")).expect("read hooks");
    let rule_writing =
        fs::read_to_string(skill_dir.join("references/rule-writing.md")).expect("read rules");
    let validation =
        fs::read_to_string(skill_dir.join("references/validation.md")).expect("read validation");

    assert!(skill.contains("name: nudge"));
    assert!(skill.contains("references/hook-responses.md"));
    assert!(skill.contains("references/rule-writing.md"));
    assert!(skill.contains("references/validation.md"));
    assert!(hook_responses.contains("Nudge Hook Responses"));
    assert!(rule_writing.contains("Nudge Rule Writing"));
    assert!(validation.contains("Nudge Validation"));

    let skill_dir = temp.path().join(".claude/skills/nudge-learnings");
    let skill = fs::read_to_string(skill_dir.join("SKILL.md")).expect("read SKILL.md");
    let bm25 = fs::read_to_string(skill_dir.join("references/bm25.md")).expect("read bm25");
    let embeddings =
        fs::read_to_string(skill_dir.join("references/embeddings.md")).expect("read embeddings");

    assert!(skill.contains("references/bm25.md"));
    assert!(skill.contains("references/embeddings.md"));
    assert!(bm25.contains("BM25 lexical search"));
    assert!(embeddings.contains("hybrid retrieval"));
}

#[test]
fn codex_skills_install_writes_agents_bundled_skills() {
    let temp = TempDir::new().expect("temp dir");

    let (exit_code, stdout, stderr) = run_nudge_in(temp.path(), &["codex", "skills", "install"]);

    pretty_assert_eq!(exit_code, 0, "skill install failed: {stderr}");
    assert!(
        stdout.contains("Installed nudge skill"),
        "install should report destination, got: {stdout}"
    );
    assert!(
        stdout.contains("Installed nudge-learnings skill"),
        "install should report destination, got: {stdout}"
    );

    let skill_dir = temp.path().join(".agents/skills/nudge");
    assert!(skill_dir.join("SKILL.md").exists());
    assert!(skill_dir.join("references/hook-responses.md").exists());
    assert!(skill_dir.join("references/rule-writing.md").exists());
    assert!(skill_dir.join("references/validation.md").exists());

    let skill_dir = temp.path().join(".agents/skills/nudge-learnings");
    assert!(skill_dir.join("SKILL.md").exists());
    assert!(skill_dir.join("references/bm25.md").exists());
    assert!(skill_dir.join("references/embeddings.md").exists());
}
