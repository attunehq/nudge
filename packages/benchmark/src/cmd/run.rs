//! Run benchmarks comparing Pavlov hooks vs CLAUDE.md guidance.

use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use color_eyre::Result;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Number of runs per (scenario, mode, model) combination.
    #[arg(short, long, default_value = "5")]
    runs: usize,

    /// Specific scenario to run (by name). Runs all if not specified.
    #[arg(short, long)]
    scenario: Option<String>,

    /// Models to test. Can be specified multiple times.
    /// Use full model IDs like: claude-3-5-haiku-20241022, claude-sonnet-4-5-20250929
    #[arg(short, long, default_value = "claude-sonnet-4-5-20250929")]
    model: Vec<String>,

    /// Only run Baseline mode.
    #[arg(long)]
    baseline_only: bool,

    /// Only run WithClaudeMd mode.
    #[arg(long)]
    claude_md_only: bool,

    /// Only run WithHooks mode.
    #[arg(long)]
    hooks_only: bool,

    /// Output file for results (JSON).
    #[arg(short, long, default_value = "benchmark-results.json")]
    output: PathBuf,

    /// Timeout per run in seconds.
    #[arg(long, default_value = "300")]
    timeout: u64,

    /// Scenarios directory.
    #[arg(long)]
    scenarios_dir: Option<PathBuf>,
}

pub fn main(config: Config) -> Result<()> {
    Ok(())
}
