//! Generate report from existing benchmark results.

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use color_eyre::eyre::Result;

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

pub fn main(_: Config) -> Result<()> {
    Ok(())
}
