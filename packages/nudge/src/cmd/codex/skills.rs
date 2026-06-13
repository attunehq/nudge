//! Install bundled Nudge skills for Codex.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use color_eyre::Result;

use crate::cmd::skill_install;

#[derive(Args, Clone, Debug)]
pub struct Config {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Clone, Debug)]
enum Commands {
    /// Install the bundled Nudge learnings skill.
    Install(InstallConfig),
}

#[derive(Args, Clone, Debug)]
struct InstallConfig {
    /// Path to the .agents directory.
    #[arg(long, default_value = ".agents")]
    agents_dir: PathBuf,
}

pub fn main(config: Config) -> Result<()> {
    match config.command {
        Commands::Install(config) => install(config),
    }
}

fn install(config: InstallConfig) -> Result<()> {
    skill_install::install_nudge_learnings("Codex", &config.agents_dir.join("skills"))?;
    Ok(())
}
