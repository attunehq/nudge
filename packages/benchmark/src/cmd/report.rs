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
    // let report = Report::load(&config.input)
    //     .wrap_err_with(|| format!("Failed to load {}", config.input.display()))?;

    // match config.format {
    //     Format::Markdown => report.print_markdown(stdout())?,
    //     Format::Csv => report.print_csv(stdout())?,
    //     Format::Chart => report.print_chart(stdout())?,
    //     Format::Json => {
    //         let json = serde_json::to_string_pretty(&report)?;
    //         println!("{}", json);
    //     }
    // }

    Ok(())
}
