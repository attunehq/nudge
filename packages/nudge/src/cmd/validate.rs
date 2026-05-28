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
        print_codex_warnings(&rules);
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
    print_codex_warnings(&rules);
    Ok(())
}

fn print_codex_warnings(loaded_rules: &[rules::Rule]) {
    if !Path::new(".codex").exists() {
        return;
    }

    for rule in loaded_rules {
        if rule.hooks_pretooluse_webfetch().next().is_some() {
            eprintln!(
                "warning: rule \"{}\" uses PreToolUse WebFetch, which Claude Code supports but Codex hooks do not currently intercept.",
                rule.name
            );
        }
    }
}
