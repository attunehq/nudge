//! Shared skill installation helpers.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{Context, Result};
use nudge::skills;

pub fn install_bundled_skills(provider: &str, skills_dir: &Path) -> Result<Vec<PathBuf>> {
    let skill_dirs = skills::install_bundled_skills(skills_dir)
        .with_context(|| format!("install {provider} bundled Nudge skill"))?;

    for (skill, skill_dir) in skills::bundled_skills().iter().zip(&skill_dirs) {
        println!("Installed {} skill to {}.", skill.name, skill_dir.display());
    }
    if let Some(removed) = skills::remove_obsolete_nudge_learnings_skill(skills_dir)? {
        println!(
            "Removed obsolete nudge-learnings skill from {}.",
            removed.display()
        );
    }
    println!(
        "Use this skill when Nudge hook messages appear, when writing rules, or when working with learned repo knowledge."
    );

    Ok(skill_dirs)
}
