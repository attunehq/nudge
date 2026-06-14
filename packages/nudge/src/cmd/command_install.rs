//! Shared slash command installation helpers.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{Context, Result};
use nudge::skills;

pub fn install_claude_commands(commands_dir: &Path) -> Result<Vec<PathBuf>> {
    let command_paths = skills::install_claude_commands(commands_dir)
        .with_context(|| "install Claude bundled Nudge slash commands")?;

    for command_path in &command_paths {
        println!(
            "Installed nudge:learn command to {}.",
            command_path.display()
        );
    }

    Ok(command_paths)
}

pub fn install_codex_prompts(prompts_dir: &Path) -> Result<Vec<PathBuf>> {
    let prompt_paths = skills::install_codex_prompts(prompts_dir)
        .with_context(|| "install Codex bundled Nudge prompt commands")?;

    for prompt_path in &prompt_paths {
        println!(
            "Installed nudge-learn prompt command to {}.",
            prompt_path.display()
        );
    }

    Ok(prompt_paths)
}
