//! Bundled Nudge skills.

use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Context, Result};
use itertools::Itertools;

pub const NUDGE_SKILL_NAME: &str = "nudge";
pub const NUDGE_LEARNINGS_SKILL_NAME: &str = "nudge-learnings";

pub struct BundledSkill {
    pub name: &'static str,
    pub files: &'static [BundledSkillFile],
}

pub struct BundledSkillFile {
    pub path: &'static str,
    pub content: &'static str,
}

const NUDGE_FILES: &[BundledSkillFile] = &[
    BundledSkillFile {
        path: "SKILL.md",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/nudge/SKILL.md"
        )),
    },
    BundledSkillFile {
        path: "references/hook-responses.md",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/nudge/references/hook-responses.md"
        )),
    },
    BundledSkillFile {
        path: "references/rule-writing.md",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/nudge/references/rule-writing.md"
        )),
    },
    BundledSkillFile {
        path: "references/validation.md",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/nudge/references/validation.md"
        )),
    },
];

const NUDGE_LEARNINGS_FILES: &[BundledSkillFile] = &[
    BundledSkillFile {
        path: "SKILL.md",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/nudge-learnings/SKILL.md"
        )),
    },
    BundledSkillFile {
        path: "references/bm25.md",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/nudge-learnings/references/bm25.md"
        )),
    },
    BundledSkillFile {
        path: "references/embeddings.md",
        content: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/skills/nudge-learnings/references/embeddings.md"
        )),
    },
];

const BUNDLED_SKILLS: &[BundledSkill] = &[
    BundledSkill {
        name: NUDGE_SKILL_NAME,
        files: NUDGE_FILES,
    },
    BundledSkill {
        name: NUDGE_LEARNINGS_SKILL_NAME,
        files: NUDGE_LEARNINGS_FILES,
    },
];

pub fn bundled_skills() -> &'static [BundledSkill] {
    BUNDLED_SKILLS
}

pub fn nudge_files() -> &'static [BundledSkillFile] {
    NUDGE_FILES
}

pub fn nudge_learnings_files() -> &'static [BundledSkillFile] {
    NUDGE_LEARNINGS_FILES
}

fn install_skill(skills_dir: &Path, skill: &BundledSkill) -> Result<PathBuf> {
    let skill_dir = skills_dir.join(skill.name);
    for file in skill.files {
        let path = skill_dir.join(file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create skill directory: {}", parent.display()))?;
        }
        fs::write(&path, file.content).with_context(|| format!("write {}", path.display()))?;
    }

    Ok(skill_dir)
}

pub fn install_bundled_skills(skills_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut installed = Vec::new();
    for skill in bundled_skills() {
        installed.push(install_skill(skills_dir, skill)?);
    }
    Ok(installed)
}

pub fn install_nudge_skill(skills_dir: &Path) -> Result<PathBuf> {
    install_skill(skills_dir, &BUNDLED_SKILLS[0])
}

pub fn install_nudge_learnings_skill(skills_dir: &Path) -> Result<PathBuf> {
    install_skill(skills_dir, &BUNDLED_SKILLS[1])
}

pub fn render_nudge_learnings_docs() -> String {
    nudge_learnings_files()
        .iter()
        .map(|file| format!("# {}\n\n{}", file.path, file.content.trim_end_matches('\n')))
        .join("\n\n")
}
