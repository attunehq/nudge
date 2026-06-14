//! Install the bundled Nudge skill for Claude Code.

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
    /// Install the bundled Nudge skill.
    Install(InstallConfig),
}

#[derive(Args, Clone, Debug)]
struct InstallConfig {
    /// Path to the .claude directory.
    #[arg(long, default_value = ".claude")]
    claude_dir: PathBuf,
}

pub fn main(config: Config) -> Result<()> {
    match config.command {
        Commands::Install(config) => install(config),
    }
}

fn install(config: InstallConfig) -> Result<()> {
    skill_install::install_bundled_skills("Claude", &config.claude_dir.join("skills"))?;
    Ok(())
}
