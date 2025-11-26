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
    // let dir = config
    //     .scenarios_dir
    //     .or_else(|| scenario::scenarios_dir().ok())
    //     .ok_or_else(|| color_eyre::eyre::eyre!("Could not find scenarios directory"))?;

    // let scenarios = Scenario::load_all(&dir)?;

    // if scenarios.is_empty() {
    //     println!("No scenarios found in {}", dir.display());
    //     return Ok(());
    // }

    // println!("Available scenarios in {}:\n", dir.display());

    // for scenario in scenarios {
    //     println!("  {} (rule: {})", scenario.name, scenario.rule);
    //     println!("    Files: {:?}", scenario.check_files);
    //     println!();
    // }

    Ok(())
}
