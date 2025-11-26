//! Run benchmarks comparing Pavlov hooks vs CLAUDE.md guidance.

use std::path::PathBuf;

use clap::Args;
use color_eyre::Result;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Number of runs per (scenario, mode, model) combination.
    #[arg(short, long, default_value = "5")]
    runs: usize,

    /// Scenarios directory.
    #[arg(long)]
    scenarios_dir: Option<PathBuf>,
}

pub fn main(config: Config) -> Result<()> {
    Ok(())
}
