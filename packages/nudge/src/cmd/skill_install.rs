//! Shared skill installation helpers.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{Context, Result};
use nudge::skills;

pub fn install_nudge_learnings(provider: &str, skills_dir: &Path) -> Result<PathBuf> {
    let skill_dir = skills::install_nudge_learnings_skill(skills_dir)
        .with_context(|| format!("install {provider} Nudge learnings skill"))?;

    println!(
        "Installed {} skill to {}.",
        skills::NUDGE_LEARNINGS_SKILL_NAME,
        skill_dir.display()
    );
    println!(
        "Use this skill when Nudge surfaces learned repo knowledge or after fixing a repo-specific issue worth recording."
    );

    Ok(skill_dir)
}
