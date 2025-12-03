//! Benchmark CLI for Nudge.
//!
//! Run benchmarks comparing Nudge hooks vs CLAUDE.md guidance.

use clap::{Parser, Subcommand};
use color_eyre::Result;
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

mod cmd;

/// Benchmark Nudge rule enforcement effectiveness.
#[derive(Parser)]
#[command(name = "benchmark", author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run benchmarks.
    Run(cmd::run::Config),

    /// Generate report from existing results.
    Report(cmd::report::Config),

    /// List available scenarios.
    List(cmd::list::Config),

    /// Display syntax tree for code.
    Syntax(cmd::syntax::Config),
}

fn main() -> Result<()> {
    color_eyre::install()?;

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
                .pretty()
                .with_writer(std::io::stderr)
                .with_filter(
                    tracing_subscriber::EnvFilter::builder()
                        .with_default_directive(LevelFilter::INFO.into())
                        .from_env_lossy(),
                ),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Run(config) => cmd::run::main(config),
        Commands::Report(config) => cmd::report::main(config),
        Commands::List(config) => cmd::list::main(config),
        Commands::Syntax(config) => cmd::syntax::main(config),
    }
}
