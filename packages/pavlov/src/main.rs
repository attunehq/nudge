//! Pavlov adds memory to your agents without burning their context.

use color_eyre::Result;
use tracing::{instrument, level_filters::LevelFilter};

mod cmd;

use clap::{Parser, Subcommand};
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Pavlov adds memory to your agents.
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Integration with Claude Code.
    Claude(cmd::claude::Config),
}

#[instrument]
fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    tracing_subscriber::registry()
        .with(ErrorLayer::default())
        .with(
            tracing_subscriber::fmt::layer()
                .with_level(true)
                .with_file(true)
                .with_line_number(true)
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .pretty(),
        )
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    match cli.command {
        Commands::Claude(config) => cmd::claude::main(config),
    }
}
