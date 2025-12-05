//! List available benchmark scenarios.

use std::path::PathBuf;

use benchmark::load_scenarios;
use clap::Args;
use color_eyre::{Result, eyre::eyre};
use color_print::cformat;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Name of a specific scenario to display.
    name: Option<String>,

    /// Scenarios directory.
    #[arg(long, default_value = "packages/benchmark/scenarios")]
    scenarios_dir: PathBuf,
}

pub fn main(config: Config) -> Result<()> {
    let scenarios = load_scenarios(&config.scenarios_dir)?;

    match &config.name {
        None => {
            println!("{}", cformat!("<bold>Available scenarios:</>"));
            for scenario in scenarios {
                let name = &scenario.name;
                let description = scenario.description.as_deref().unwrap_or("No description");
                println!(
                    "  {}",
                    cformat!("<cyan>-</> <green>{name}</>: <dim>{description}</>")
                );
            }
        }
        Some(name) => {
            let scenario = scenarios
                .into_iter()
                .find(|s| s.name == *name)
                .ok_or_else(|| eyre!("scenario not found: {name}"))?;
            print!("{scenario}");
        }
    }

    Ok(())
}
