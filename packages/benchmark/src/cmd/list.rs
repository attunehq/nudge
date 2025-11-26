//! List available benchmark scenarios.

use std::path::PathBuf;

use clap::Args;
use color_eyre::Result;

use benchmark::scenario::{self, Scenario};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Scenarios directory.
    #[arg(long)]
    scenarios_dir: Option<PathBuf>,
}

pub fn main(config: Config) -> Result<()> {
    Ok(())
}
