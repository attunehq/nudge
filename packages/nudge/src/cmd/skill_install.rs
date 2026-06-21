//! Shared skill installation helpers.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{Context, Result};
use nudge::skills;

pub fn install_bundled_skills(provider: &str, skills_dir: &Path) -> Result<Vec<PathBuf>> {
    let skill_dirs = skills::install_bundled_skills(skills_dir)
        .with_context(|| format!("install {provider} bundled Nudge skills"))?;

    for (skill, skill_dir) in skills::bundled_skills().iter().zip(&skill_dirs) {
        println!("Installed {} skill to {}.", skill.name, skill_dir.display());
    }
    println!(
        "Use these skills when Nudge hook messages appear, when writing rules, or when working with learned repo knowledge."
    );

    Ok(skill_dirs)
}
