//! Validate rule configuration files.

use std::path::{Path, PathBuf};

use clap::Args;
use color_eyre::eyre::{Context, Result};
use nudge::rules;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to a specific config file to validate.
    /// If not specified, validates all discoverable config files.
    pub path: Option<PathBuf>,
}

pub fn main(config: Config) -> Result<()> {
    match config.path {
        Some(path) => validate_file(&path),
        None => validate_all(),
    }
}

/// Validate all discoverable config files.
fn validate_all() -> Result<()> {
    let rules = rules::load_all_attributed().context("load rules")?;
    for (path, rules) in rules {
        let yaml = serde_yaml::to_string(&rules).context("serialize rules")?;
        println!("Config file: {path:?}");
        println!("{yaml}");
        println!("------");
        println!();
    }

    Ok(())
}

/// Validate a single config file and print results.
fn validate_file(path: &Path) -> Result<()> {
    let rules = rules::load_from(path).context("parse rules file")?;
    let yaml = serde_yaml::to_string(&rules).context("serialize rules")?;
    println!("{yaml}");
    Ok(())
}
