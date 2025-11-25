//! Integration with Claude Code.

use clap::{Args, Subcommand};
use color_eyre::Result;
use tracing::instrument;

pub mod hook;
pub mod setup;

#[derive(Args, Clone, Debug)]
pub struct Config {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Clone, Debug)]
enum Commands {
    /// Responds to Claude Code hooks.
    Hook(hook::Config),

    /// Set up Pavlov hooks in .claude/hooks.
    Setup(setup::Config),
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    match config.command {
        Commands::Hook(config) => hook::main(config),
        Commands::Setup(config) => setup::main(config),
    }
}
