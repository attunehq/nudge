//! Run benchmarks comparing Pavlov hooks vs CLAUDE.md guidance.

use std::io::stdout;
use std::path::PathBuf;
use std::time::Duration;

use clap::Args;
use color_eyre::Result;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Number of runs per (scenario, mode, model) combination.
    #[arg(short, long, default_value = "5")]
    runs: usize,

    /// Specific scenario to run (by name). Runs all if not specified.
    #[arg(short, long)]
    scenario: Option<String>,

    /// Models to test. Can be specified multiple times.
    /// Use full model IDs like: claude-3-5-haiku-20241022, claude-sonnet-4-5-20250929
    #[arg(short, long, default_value = "claude-sonnet-4-5-20250929")]
    model: Vec<String>,

    /// Only run Baseline mode.
    #[arg(long)]
    baseline_only: bool,

    /// Only run WithClaudeMd mode.
    #[arg(long)]
    claude_md_only: bool,

    /// Only run WithHooks mode.
    #[arg(long)]
    hooks_only: bool,

    /// Output file for results (JSON).
    #[arg(short, long, default_value = "benchmark-results.json")]
    output: PathBuf,

    /// Timeout per run in seconds.
    #[arg(long, default_value = "300")]
    timeout: u64,

    /// Scenarios directory.
    #[arg(long)]
    scenarios_dir: Option<PathBuf>,
}

pub fn main(config: Config) -> Result<()> {
    // // Find scenarios
    // let dir = config
    //     .scenarios_dir
    //     .or_else(|| scenario::scenarios_dir().ok())
    //     .ok_or_else(|| color_eyre::eyre::eyre!("Could not find scenarios directory"))?;

    // let scenarios = Scenario::load_all(&dir)?;

    // if scenarios.is_empty() {
    //     tracing::warn!("No scenarios found in {}", dir.display());
    //     return Ok(());
    // }

    // // Filter scenarios if requested
    // let scenarios: Vec<_> = if let Some(ref filter) = config.scenario {
    //     scenarios
    //         .into_iter()
    //         .filter(|s| s.name.contains(filter))
    //         .collect()
    // } else {
    //     scenarios
    // };

    // if scenarios.is_empty() {
    //     tracing::error!("No scenarios match filter: {:?}", config.scenario);
    //     return Ok(());
    // }

    // // Determine modes to run
    // let modes: Vec<Mode> = match (
    //     config.baseline_only,
    //     config.claude_md_only,
    //     config.hooks_only,
    // ) {
    //     (true, false, false) => vec![Mode::Baseline],
    //     (false, true, false) => vec![Mode::WithClaudeMd],
    //     (false, false, true) => vec![Mode::WithHooks],
    //     _ => vec![Mode::Baseline, Mode::WithClaudeMd, Mode::WithHooks],
    // };

    // // Find pavlov binary
    // let pavlov_bin = runner::find_pavlov_bin()?;
    // tracing::info!("Using pavlov binary: {}", pavlov_bin.display());

    // let timeout = Duration::from_secs(config.timeout);

    // // Calculate total runs
    // let total_runs = scenarios.len() * modes.len() * config.model.len() * config.runs;
    // tracing::info!(
    //     "Running {} total benchmarks ({} scenarios × {} modes × {} models × {} runs each)",
    //     total_runs,
    //     scenarios.len(),
    //     modes.len(),
    //     config.model.len(),
    //     config.runs
    // );

    // let mut results = Vec::new();
    // let mut completed = 0;

    // for scenario in &scenarios {
    //     for mode in &modes {
    //         for model in &config.model {
    //             for run_id in 0..config.runs {
    //                 completed += 1;

    //                 tracing::info!(
    //                     "[{}/{}] {} | {} | {} | run {}",
    //                     completed,
    //                     total_runs,
    //                     scenario.name,
    //                     mode,
    //                     shorten_model(model),
    //                     run_id + 1
    //                 );

    //                 let run_config = RunConfig {
    //                     scenario,
    //                     mode: *mode,
    //                     model,
    //                     run_id,
    //                     pavlov_bin: &pavlov_bin,
    //                     timeout,
    //                 };

    //                 match runner::execute_run(&run_config) {
    //                     Ok(result) => {
    //                         let status = if result.rule_followed { "✓" } else { "✗" };
    //                         tracing::info!(
    //                             "  {} rule_followed={} duration={}ms",
    //                             status,
    //                             result.rule_followed,
    //                             result.duration_ms
    //                         );
    //                         results.push(result);
    //                     }
    //                     Err(e) => {
    //                         tracing::error!("  Run failed: {}", e);
    //                     }
    //                 }

    //                 // Save intermediate results
    //                 if completed % 10 == 0 {
    //                     let report = Report::from_results(results.clone());
    //                     if let Err(e) = report.save(&config.output) {
    //                         tracing::warn!("Failed to save intermediate results: {}", e);
    //                     }
    //                 }
    //             }
    //         }
    //     }
    // }

    // // Save final results
    // let report = Report::from_results(results);
    // report.save(&config.output)?;
    // tracing::info!("Results saved to {}", config.output.display());

    // // Print summary
    // println!();
    // report.print_chart(stdout())?;
    // report.print_markdown(stdout())?;

    Ok(())
}

fn shorten_model(model: &str) -> &str {
    if model.contains("opus") {
        "opus"
    } else if model.contains("sonnet") {
        "sonnet"
    } else if model.contains("haiku") {
        "haiku"
    } else {
        model
    }
}
