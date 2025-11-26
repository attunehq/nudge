//! Generate report from existing benchmark results.

use std::io::stdout;
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use color_eyre::eyre::{Result, WrapErr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Markdown,
    Csv,
    Chart,
    Json,
}

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Input results file (JSON).
    #[arg(short, long, default_value = "benchmark-results.json")]
    input: PathBuf,

    /// Output format.
    #[arg(short, long, default_value = "markdown")]
    format: Format,
}

pub fn main(config: Config) -> Result<()> {
    Ok(())
}
