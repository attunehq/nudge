//! Integration with Grok Build.

use clap::{Args, Subcommand};
use color_eyre::Result;
use tracing::instrument;

pub mod hook;
pub mod setup;
pub mod skills;

#[derive(Args, Clone, Debug)]
pub struct Config {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Clone, Debug)]
enum Commands {
    /// Responds to Grok Build hooks.
    Hook(hook::Config),

    /// Set up Nudge hooks in .grok/hooks/nudge.json, backing up existing hooks.
    Setup(setup::Config),

    /// Install the bundled Nudge skills into .grok/skills.
    Skills(skills::Config),
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    match config.command {
        Commands::Hook(config) => hook::main(config),
        Commands::Setup(config) => setup::main(config),
        Commands::Skills(config) => skills::main(config),
    }
}
