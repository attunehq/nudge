//! Install the bundled Nudge skills for Grok Build.

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
    /// Install the bundled Nudge skills.
    Install(InstallConfig),
}

#[derive(Args, Clone, Debug)]
struct InstallConfig {
    /// Path to the .grok directory.
    #[arg(long, default_value = ".grok")]
    grok_dir: PathBuf,
}

pub fn main(config: Config) -> Result<()> {
    match config.command {
        Commands::Install(config) => install(config),
    }
}

fn install(config: InstallConfig) -> Result<()> {
    skill_install::install_bundled_skills("Grok", &config.grok_dir.join("skills"))?;
    Ok(())
}
