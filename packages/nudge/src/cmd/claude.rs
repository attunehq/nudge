//! Integration with Claude Code.

use clap::{Args, Subcommand};
use color_eyre::Result;
use tracing::instrument;

pub mod docs;
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
    /// Responds to Claude Code hooks.
    Hook(hook::Config),

    /// Set up Nudge hooks in .claude/settings.local.json, backing up existing
    /// settings.
    Setup(setup::Config),

    /// Show documentation for writing Nudge rules.
    Docs(docs::Config),

    /// Install the bundled Nudge skill into .claude/skills.
    Skills(skills::Config),
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    match config.command {
        Commands::Hook(config) => hook::main(config),
        Commands::Setup(config) => setup::main(config),
        Commands::Docs(config) => docs::main(config),
        Commands::Skills(config) => skills::main(config),
    }
}
