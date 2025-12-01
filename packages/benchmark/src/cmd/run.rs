//! Run benchmarks comparing Pavlov hooks vs CLAUDE.md guidance.

use std::path::PathBuf;

use benchmark::{Agent, Guidance, ModelClaudeCode, Scenario, evaluate, load_scenarios};
use clap::Args;
use color_eyre::{Result, eyre::eyre};
use color_print::{cformat, cprintln};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Number of runs per (scenario, agent, guidance) combination.
    #[arg(short, long, default_value = "5")]
    runs: usize,

    /// Show what would be run without actually running benchmarks.
    #[arg(long)]
    dry_run: bool,

    /// Agents to evaluate (can be specified multiple times).
    ///
    /// Format: `claude-code:<model>` where model is `sonnet`, `haiku`, `opus`,
    /// or a full model ID like `claude-sonnet-4-5-20250929`.
    ///
    /// Example: `-a claude-code:sonnet -a claude-code:opus`
    #[arg(short, long = "agent")]
    agents: Vec<Agent>,

    /// Guidance modes to test (can be specified multiple times).
    ///
    /// Example: `-g none -g pavlov -g file`
    #[arg(short, long = "guidance", value_enum)]
    guidances: Vec<Guidance>,

    /// Scenarios to run (can be specified multiple times).
    ///
    /// If not specified, all scenarios are run.
    ///
    /// Example: `-s field_spacing -s lhs_annotations`
    #[arg(short, long = "scenario")]
    scenarios: Vec<String>,

    /// Scenarios directory.
    #[arg(long, default_value = "packages/benchmark/scenarios")]
    scenarios_dir: PathBuf,
}

/// Fully resolved benchmark configuration with defaults applied.
struct ResolvedConfig {
    runs: usize,
    scenarios: Vec<Scenario>,
    agents: Vec<Agent>,
    guidances: Vec<Guidance>,
}

impl ResolvedConfig {
    fn combinations(&self) -> usize {
        self.scenarios.len() * self.agents.len() * self.guidances.len()
    }

    fn total_runs(&self) -> usize {
        self.combinations() * self.runs
    }

    fn print_summary(&self) {
        println!("{}", cformat!("<bold,underline>Benchmark Configuration</>"));
        let scenarios = self
            .scenarios
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {}", cformat!("<cyan>Scenarios:</> {scenarios}"));
        let agents = self
            .agents
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {}", cformat!("<cyan>Agents:</> {agents}"));
        let guidances = self
            .guidances
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {}", cformat!("<cyan>Guidance modes:</> {guidances}"));
        let runs = self.runs;
        let combinations = self.combinations();
        let total = self.total_runs();
        println!(
            "  {}",
            cformat!("<cyan>Runs:</> {runs} runs × {combinations} combinations = {total} total")
        );
        println!();
    }
}

pub fn main(config: Config) -> Result<()> {
    let dry_run = config.dry_run;
    let resolved = ResolvedConfig::try_from(config)?;

    resolved.print_summary();

    if dry_run {
        main_dry_run(&resolved)
    } else {
        main_run(&resolved)
    }
}

fn main_dry_run(config: &ResolvedConfig) -> Result<()> {
    println!(
        "{}",
        cformat!("<yellow,bold>Dry run: listing all combinations:</>")
    );

    let mut run_number = 0;
    let total_runs = config.total_runs();
    let runs = config.runs;
    for scenario in &config.scenarios {
        for agent in &config.agents {
            for guidance in &config.guidances {
                for run in 1..=runs {
                    run_number += 1;
                    let scenario_name = &scenario.name;
                    let agent_name = agent.name();
                    let model = agent.model();
                    let guidance_str = guidance.to_string();
                    println!(
                        "  [{run_number}/{total_runs}] {}",
                        cformat!(
                            "<green,bold>Run:</> <dim>{run}/{runs}</> × \
                            <green,bold>Scenario:</> <dim>{scenario_name}</> × \
                            <green,bold>Agent:</> <dim>{agent_name}</> × \
                            <green,bold>Model:</> <dim>{model}</> × \
                            <green,bold>Guidance:</> <dim>{guidance_str}</>"
                        )
                    );
                }
            }
        }
    }

    Ok(())
}

fn main_run(config: &ResolvedConfig) -> Result<()> {
    let mut run_number = 0;
    let total_runs = config.total_runs();
    let runs = config.runs;

    for scenario in &config.scenarios {
        for agent in &config.agents {
            for guidance in &config.guidances {
                for run in 1..=runs {
                    run_number += 1;
                    let scenario_name = &scenario.name;
                    let agent_name = agent.name();
                    let model = agent.model();
                    let guidance_str = guidance.to_string();
                    println!(
                        "{} [{run_number}/{total_runs}] {}",
                        cformat!("<green,bold>Running</>"),
                        cformat!(
                            "<green,bold>Run:</> <dim>{run}/{runs}</> × \
                            <green,bold>Scenario:</> <dim>{scenario_name}</> × \
                            <green,bold>Agent:</> <dim>{agent_name}</> × \
                            <green,bold>Model:</> <dim>{model}</> × \
                            <green,bold>Guidance:</> <dim>{guidance_str}</>"
                        )
                    );

                    match evaluate(scenario, agent, *guidance) {
                        Ok(outcome) => {
                            println!("  {outcome}");
                        }
                        Err(e) => {
                            cprintln!("<yellow>⚠</> Error: {e}");
                        }
                    }
                }
            }
        }
    }

    println!();
    println!("{}", cformat!("<bold>Benchmark complete.</>"));

    Ok(())
}

impl TryFrom<Config> for ResolvedConfig {
    type Error = color_eyre::eyre::Error;

    fn try_from(config: Config) -> Result<Self> {
        let all_scenarios = load_scenarios(&config.scenarios_dir)?;

        let scenarios = if config.scenarios.is_empty() {
            all_scenarios
        } else {
            let mut filtered = Vec::new();
            for name in &config.scenarios {
                let scenario = all_scenarios
                    .iter()
                    .find(|s| s.name == *name)
                    .ok_or_else(|| eyre!("scenario not found: {name}"))?
                    .clone();
                filtered.push(scenario);
            }
            filtered
        };

        let agents = if config.agents.is_empty() {
            vec![Agent::ClaudeCode(ModelClaudeCode::SonnetLatest)]
        } else {
            config.agents
        };

        let guidances = if config.guidances.is_empty() {
            vec![Guidance::None, Guidance::Pavlov, Guidance::File]
        } else {
            config.guidances
        };

        Ok(Self {
            runs: config.runs,
            scenarios,
            agents,
            guidances,
        })
    }
}
