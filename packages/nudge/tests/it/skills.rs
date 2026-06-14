//! Bundled skill installation tests.

use std::{fs, path::Path, process::Command};

use crate::nudge_binary;
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
fn claude_skills_install_writes_bundled_skills() {
    let temp = TempDir::new().expect("temp dir");
    let obsolete_skill = temp.path().join(".claude/skills/nudge-learnings");
    fs::create_dir_all(&obsolete_skill).expect("create obsolete skill");
    fs::write(
        obsolete_skill.join("SKILL.md"),
        "---\nname: nudge-learnings\n---\n",
    )
    .expect("write obsolete skill");

    let (exit_code, stdout, stderr) = run_nudge_in(temp.path(), &["claude", "skills", "install"]);

    pretty_assert_eq!(exit_code, 0, "skill install failed: {stderr}");
    assert!(
        stdout.contains("Installed nudge skill"),
        "install should report destination, got: {stdout}"
    );
    assert!(
        !stdout.contains("Installed nudge-learnings skill"),
        "install should not report obsolete standalone learnings skill, got: {stdout}"
    );
    assert!(
        stdout.contains("Removed obsolete nudge-learnings skill"),
        "install should report stale skill cleanup, got: {stdout}"
    );

    let skill_dir = temp.path().join(".claude/skills/nudge");
    let skill = fs::read_to_string(skill_dir.join("SKILL.md")).expect("read SKILL.md");
    let ci = fs::read_to_string(skill_dir.join("references/ci.md")).expect("read ci");
    let hook_responses =
        fs::read_to_string(skill_dir.join("references/hook-responses.md")).expect("read hooks");
    let learnings =
        fs::read_to_string(skill_dir.join("references/learnings.md")).expect("read learnings");
    let learnings_bm25 = fs::read_to_string(skill_dir.join("references/learnings-bm25.md"))
        .expect("read learnings bm25");
    let learnings_embeddings =
        fs::read_to_string(skill_dir.join("references/learnings-embeddings.md"))
            .expect("read learnings embeddings");
    let rule_debugging = fs::read_to_string(skill_dir.join("references/rule-debugging.md"))
        .expect("read rule debugging");
    let rule_writing =
        fs::read_to_string(skill_dir.join("references/rule-writing.md")).expect("read rules");
    let setup = fs::read_to_string(skill_dir.join("references/setup.md")).expect("read setup");
    let validation =
        fs::read_to_string(skill_dir.join("references/validation.md")).expect("read validation");

    assert!(skill.contains("name: nudge"));
    assert!(skill.contains("references/ci.md"));
    assert!(skill.contains("references/hook-responses.md"));
    assert!(skill.contains("references/learnings.md"));
    assert!(skill.contains("references/learnings-bm25.md"));
    assert!(skill.contains("references/learnings-embeddings.md"));
    assert!(skill.contains("references/setup.md"));
    assert!(skill.contains("references/rule-debugging.md"));
    assert!(skill.contains("references/rule-writing.md"));
    assert!(skill.contains("references/validation.md"));
    assert!(ci.contains("Nudge CI"));
    assert!(hook_responses.contains("Nudge Hook Responses"));
    assert!(hook_responses.contains("PreToolUse WebFetch"));
    assert!(hook_responses.contains("apply_patch"));
    assert!(learnings.contains("Nudge Learnings"));
    assert!(learnings_bm25.contains("Nudge Learnings Without Embeddings"));
    assert!(learnings_embeddings.contains("Nudge Learnings With Local Embeddings"));
    assert!(rule_debugging.contains("Nudge Rule Debugging"));
    assert!(rule_writing.contains("Nudge Rule Writing"));
    assert!(rule_writing.contains("MarkdownCodeBlock"));
    assert!(rule_writing.contains("SyntaxTree"));
    assert!(rule_writing.contains("External"));
    assert!(rule_writing.contains("project_state"));
    assert!(rule_writing.contains("UserPromptSubmit"));
    assert!(rule_writing.contains("(?m)"));
    assert!(rule_writing.contains("{{ $suggestion }}"));
    assert!(!rule_writing.contains("Contains"));
    assert!(!rule_writing.contains("nudge claude docs"));
    assert!(!rule_writing.contains("nudge codex docs"));
    assert!(!rule_writing.contains("nudge learn docs"));
    assert!(setup.contains("Nudge Setup"));
    assert!(setup.contains("nudge claude setup"));
    assert!(setup.contains("nudge codex setup"));
    assert!(setup.contains("nudge claude skills install"));
    assert!(setup.contains("Do not edit `CLAUDE.md`, `AGENTS.md`"));
    assert!(validation.contains("Nudge Validation"));
    assert!(
        !temp.path().join(".claude/skills/nudge-learnings").exists(),
        "install should not create obsolete standalone learnings skill"
    );
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
        !stdout.contains("Installed nudge-learnings skill"),
        "install should not report obsolete standalone learnings skill, got: {stdout}"
    );

    let skill_dir = temp.path().join(".agents/skills/nudge");
    assert!(skill_dir.join("SKILL.md").exists());
    assert!(skill_dir.join("references/ci.md").exists());
    assert!(skill_dir.join("references/hook-responses.md").exists());
    assert!(skill_dir.join("references/learnings.md").exists());
    assert!(skill_dir.join("references/learnings-bm25.md").exists());
    assert!(
        skill_dir
            .join("references/learnings-embeddings.md")
            .exists()
    );
    assert!(skill_dir.join("references/setup.md").exists());
    assert!(skill_dir.join("references/rule-debugging.md").exists());
    assert!(skill_dir.join("references/rule-writing.md").exists());
    assert!(skill_dir.join("references/validation.md").exists());
    assert!(
        !temp.path().join(".agents/skills/nudge-learnings").exists(),
        "install should not create obsolete standalone learnings skill"
    );
}

#[test]
fn docs_subcommands_are_removed_from_help() {
    let temp = TempDir::new().expect("temp dir");

    for args in [
        &["claude", "--help"][..],
        &["codex", "--help"][..],
        &["learn", "--help"][..],
    ] {
        let (exit_code, stdout, stderr) = run_nudge_in(temp.path(), args);

        pretty_assert_eq!(exit_code, 0, "{args:?} failed: {stderr}");
        assert!(
            !stdout.contains("docs"),
            "{args:?} help should not advertise docs subcommands, got: {stdout}"
        );
    }
}
