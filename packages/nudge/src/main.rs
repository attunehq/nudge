//! Nudge adds memory to your agents without burning their context.

use color_eyre::{Result, Section};
use tracing::{instrument, level_filters::LevelFilter};

mod cmd;

use clap::{Parser, Subcommand};
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Nudge adds memory to your agents.
#[derive(Parser)]
#[command(author, version = env!("NUDGE_VERSION"), about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check project files against configured rules.
    Check(cmd::check::Config),

    /// Integration with Claude Code.
    Claude(cmd::claude::Config),

    /// Display the syntax tree for code (for writing tree-sitter queries).
    Syntaxtree(cmd::syntaxtree::Config),

    /// Validate rule configuration files.
    Validate(cmd::validate::Config),

    /// Test a rule against sample input.
    Test(cmd::test::Config),
}

#[instrument]
fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    // Note: in normal operation, Claude Code invokes `nudge claude hook` as a
    // subprocess, which gives it access to `stderr` if the hook exits with an
    // "interrupt" response. This means that by default, we only log warnings;
    // the intention with tracing usage in this binary is to support manual
    // debugging with `NUDGE_LOG` directives.
    //
    // Examples:
    // - `NUDGE_LOG=trace` to log all messages
    // - `NUDGE_LOG=debug` to log debug, info, warn, and error messages
    // - `NUDGE_LOG=info` to log info, warn, and error messages
    // - `NUDGE_LOG=warn` to log warn and error messages (this is the default)
    // - `NUDGE_LOG=error` to log only error messages
    tracing_subscriber::registry()
        .with(ErrorLayer::default())
        .with(
            fmt::layer()
                .with_level(true)
                .with_file(true)
                .with_line_number(true)
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .pretty(),
        )
        .with(
            EnvFilter::builder()
                .with_env_var("NUDGE_LOG")
                .with_default_directive(LevelFilter::ERROR.into())
                .from_env_lossy(),
        )
        .init();

    // The suggestion is only added if the command fails; the intention here is
    // that users or claude code can see an error and then run the command to
    // learn more about debugging nudge.
    match cli.command {
        Commands::Check(config) => cmd::check::main(config),
        Commands::Claude(config) => cmd::claude::main(config),
        Commands::Syntaxtree(config) => cmd::syntaxtree::main(config),
        Commands::Validate(config) => cmd::validate::main(config),
        Commands::Test(config) => cmd::test::main(config),
    }
    .suggestion("Run `nudge claude docs` for documentation on writing/debugging Claude Code rules.")
}
