//! List available benchmark scenarios.

use std::{fs::read_to_string, path::PathBuf};

use benchmark::Scenario;
use clap::Args;
use color_eyre::{
    Result,
    eyre::{Context, eyre},
};
use owo_colors::OwoColorize;

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
            println!("{}", "Available scenarios:".bold());
            for scenario in scenarios {
                let description = scenario
                    .description
                    .as_deref()
                    .unwrap_or("No description");
                println!(
                    "  {} {}: {}",
                    "-".cyan(),
                    scenario.name.green(),
                    description.dimmed()
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

fn load_scenarios(dir: &PathBuf) -> Result<Vec<Scenario>> {
    let mut scenarios = Vec::new();

    let entries = dir
        .read_dir()
        .with_context(|| format!("read scenarios directory: {dir:?}"))?;

    for entry in entries {
        let entry = entry.context("read directory entry")?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "toml") {
            let content = read_to_string(&path)
                .with_context(|| format!("read scenario file: {path:?}"))?;
            let scenario = toml::from_str::<Scenario>(&content)
                .with_context(|| format!("parse scenario file: {path:?}"))?;
            scenarios.push(scenario);
        }
    }

    scenarios.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(scenarios)
}
