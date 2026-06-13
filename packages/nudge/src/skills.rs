//! Bundled Nudge skills.

use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Context, Result};
use itertools::Itertools;

pub const NUDGE_LEARNINGS_SKILL_NAME: &str = "nudge-learnings";

pub struct BundledSkillFile {
    pub path: &'static str,
    pub content: &'static str,
}

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

pub fn nudge_learnings_files() -> &'static [BundledSkillFile] {
    NUDGE_LEARNINGS_FILES
}

pub fn install_nudge_learnings_skill(skills_dir: &Path) -> Result<PathBuf> {
    let skill_dir = skills_dir.join(NUDGE_LEARNINGS_SKILL_NAME);
    for file in nudge_learnings_files() {
        let path = skill_dir.join(file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create skill directory: {}", parent.display()))?;
        }
        fs::write(&path, file.content).with_context(|| format!("write {}", path.display()))?;
    }

    Ok(skill_dir)
}

pub fn render_nudge_learnings_docs() -> String {
    nudge_learnings_files()
        .iter()
        .map(|file| format!("# {}\n\n{}", file.path, file.content.trim_end_matches('\n')))
        .join("\n\n")
}
