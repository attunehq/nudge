//! Reporter for aggregating and displaying benchmark results.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use color_eyre::eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};

use crate::runner::{Mode, RunResult};

/// Aggregated statistics for a (scenario, mode, model) combination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedStats {
    pub scenario: String,

    pub rule: String,

    pub mode: Mode,

    pub model: String,

    pub total_runs: usize,

    pub passed: usize,

    pub failed: usize,

    pub errors: usize,

    pub pass_rate: f64,

    pub avg_duration_ms: f64,
}

/// Full benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub generated_at: String,

    pub stats: Vec<AggregatedStats>,

    pub raw_results: Vec<RunResult>,
}

impl Report {
    /// Create a report from raw results.
    pub fn from_results(results: Vec<RunResult>) -> Self {
        let stats = aggregate_results(&results);

        Report {
            generated_at: chrono_lite_now(),
            stats,
            raw_results: results,
        }
    }

    /// Load a report from JSON file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read report: {}", path.display()))?;

        serde_json::from_str(&content).wrap_err("Failed to parse report JSON")
    }

    /// Save report to JSON file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)
            .wrap_err_with(|| format!("Failed to write report: {}", path.display()))
    }

    /// Print report as markdown table.
    pub fn print_markdown<W: Write>(&self, mut w: W) -> Result<()> {
        writeln!(w, "# Pavlov Benchmark Results")?;
        writeln!(w)?;
        writeln!(w, "Generated: {}", self.generated_at)?;
        writeln!(w)?;

        // Group by scenario
        let mut by_scenario: HashMap<&str, Vec<&AggregatedStats>> = HashMap::new();
        for stat in &self.stats {
            by_scenario
                .entry(&stat.scenario)
                .or_default()
                .push(stat);
        }

        // Get all models
        let mut models: Vec<&str> = self
            .stats
            .iter()
            .map(|s| s.model.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        models.sort();

        // Print table header
        write!(w, "| Scenario | Mode |")?;
        for model in &models {
            let short_name = shorten_model_name(model);
            write!(w, " {} |", short_name)?;
        }
        writeln!(w)?;

        // Separator
        write!(w, "|----------|------|")?;
        for _ in &models {
            write!(w, "--------|")?;
        }
        writeln!(w)?;

        // Print rows
        let mut scenarios: Vec<&str> = by_scenario.keys().copied().collect();
        scenarios.sort();

        for scenario in scenarios {
            let stats = &by_scenario[scenario];

            for mode in [Mode::Baseline, Mode::WithClaudeMd, Mode::WithHooks] {
                let mode_str = match mode {
                    Mode::Baseline => "Baseline",
                    Mode::WithClaudeMd => "CLAUDE.md",
                    Mode::WithHooks => "Hooks",
                };

                write!(w, "| {} | {} |", scenario, mode_str)?;

                for model in &models {
                    let stat = stats
                        .iter()
                        .find(|s| s.mode == mode && s.model == *model);

                    if let Some(s) = stat {
                        let pct = (s.pass_rate * 100.0).round() as i32;
                        write!(w, " {}% ({}/{}) |", pct, s.passed, s.total_runs)?;
                    } else {
                        write!(w, " - |")?;
                    }
                }
                writeln!(w)?;
            }
        }

        writeln!(w)?;

        // Print summary
        self.print_summary(&mut w)?;

        Ok(())
    }

    /// Print summary statistics.
    fn print_summary<W: Write>(&self, w: &mut W) -> Result<()> {
        writeln!(w, "## Summary")?;
        writeln!(w)?;

        // Calculate averages by model and mode
        // (baseline_sum, claude_md_sum, hooks_sum, count)
        let mut by_model: HashMap<&str, (f64, f64, f64, usize)> = HashMap::new();

        for stat in &self.stats {
            let entry = by_model
                .entry(&stat.model)
                .or_insert((0.0, 0.0, 0.0, 0));

            match stat.mode {
                Mode::Baseline => entry.0 += stat.pass_rate,
                Mode::WithClaudeMd => entry.1 += stat.pass_rate,
                Mode::WithHooks => entry.2 += stat.pass_rate,
            }
            entry.3 += 1;
        }

        writeln!(
            w,
            "| Model | Baseline | CLAUDE.md | Hooks | CLAUDE.md vs Baseline | Hooks vs Baseline |"
        )?;
        writeln!(
            w,
            "|-------|----------|-----------|-------|----------------------|-------------------|"
        )?;

        let mut models: Vec<&&str> = by_model.keys().collect();
        models.sort();

        for model in models {
            let (baseline_sum, claude_md_sum, hooks_sum, count) = by_model[model];
            let scenarios_per_mode = count / 3; // 3 modes now

            if scenarios_per_mode > 0 {
                let baseline_avg = baseline_sum / scenarios_per_mode as f64;
                let claude_md_avg = claude_md_sum / scenarios_per_mode as f64;
                let hooks_avg = hooks_sum / scenarios_per_mode as f64;

                let claude_md_improvement = claude_md_avg - baseline_avg;
                let hooks_improvement = hooks_avg - baseline_avg;

                let short_name = shorten_model_name(model);
                writeln!(
                    w,
                    "| {} | {:.0}% | {:.0}% | {:.0}% | {:+.0}% | {:+.0}% |",
                    short_name,
                    baseline_avg * 100.0,
                    claude_md_avg * 100.0,
                    hooks_avg * 100.0,
                    claude_md_improvement * 100.0,
                    hooks_improvement * 100.0
                )?;
            }
        }

        Ok(())
    }

    /// Print report as CSV.
    pub fn print_csv<W: Write>(&self, mut w: W) -> Result<()> {
        writeln!(
            w,
            "scenario,rule,mode,model,total_runs,passed,failed,errors,pass_rate,avg_duration_ms"
        )?;

        for stat in &self.stats {
            writeln!(
                w,
                "{},{},{},{},{},{},{},{},{:.4},{:.2}",
                stat.scenario,
                stat.rule,
                stat.mode,
                stat.model,
                stat.total_runs,
                stat.passed,
                stat.failed,
                stat.errors,
                stat.pass_rate,
                stat.avg_duration_ms
            )?;
        }

        Ok(())
    }

    /// Print ASCII progress bar chart.
    pub fn print_chart<W: Write>(&self, mut w: W) -> Result<()> {
        writeln!(w, "\n📊 Pass Rate by Scenario and Mode\n")?;

        let mut by_scenario: HashMap<&str, Vec<&AggregatedStats>> = HashMap::new();
        for stat in &self.stats {
            by_scenario.entry(&stat.scenario).or_default().push(stat);
        }

        let mut scenarios: Vec<&str> = by_scenario.keys().copied().collect();
        scenarios.sort();

        for scenario in scenarios {
            writeln!(w, "{}:", scenario)?;

            let stats = &by_scenario[scenario];
            let mut sorted_stats = stats.clone();
            sorted_stats.sort_by(|a, b| {
                a.model
                    .cmp(&b.model)
                    .then(a.mode.to_string().cmp(&b.mode.to_string()))
            });

            for stat in sorted_stats {
                let short_model = shorten_model_name(&stat.model);
                let (mode_str, icon) = match stat.mode {
                    Mode::Baseline => ("baseline ", "⚪"),
                    Mode::WithClaudeMd => ("claude.md", "📄"),
                    Mode::WithHooks => ("hooks    ", "🔒"),
                };

                let pct = (stat.pass_rate * 100.0).round() as i32;
                let bar_len = (stat.pass_rate * 30.0).round() as usize;
                let bar = "█".repeat(bar_len);
                let empty = "░".repeat(30 - bar_len);

                writeln!(
                    w,
                    "  {:<8} {}: {} {}{} {:>3}%",
                    short_model, mode_str, icon, bar, empty, pct
                )?;
            }
            writeln!(w)?;
        }

        Ok(())
    }
}

/// Aggregate raw results into statistics.
fn aggregate_results(results: &[RunResult]) -> Vec<AggregatedStats> {
    // Group by (scenario, mode, model)
    let mut groups: HashMap<(&str, Mode, &str), Vec<&RunResult>> = HashMap::new();

    for result in results {
        let key = (result.scenario.as_str(), result.mode, result.model.as_str());
        groups.entry(key).or_default().push(result);
    }

    let mut stats = Vec::new();

    for ((scenario, mode, model), group) in groups {
        let total_runs = group.len();
        let passed = group.iter().filter(|r| r.rule_followed).count();
        let errors = group.iter().filter(|r| !r.completed).count();
        let failed = total_runs.saturating_sub(passed).saturating_sub(errors);

        let pass_rate = if total_runs > 0 {
            passed as f64 / total_runs as f64
        } else {
            0.0
        };

        let avg_duration_ms = if total_runs > 0 {
            group.iter().map(|r| r.duration_ms).sum::<u64>() as f64 / total_runs as f64
        } else {
            0.0
        };

        let rule = group.first().map(|r| r.rule.clone()).unwrap_or_default();

        stats.push(AggregatedStats {
            scenario: scenario.to_string(),
            rule,
            mode,
            model: model.to_string(),
            total_runs,
            passed,
            failed,
            errors,
            pass_rate,
            avg_duration_ms,
        });
    }

    // Sort for consistent output
    stats.sort_by(|a, b| {
        a.scenario
            .cmp(&b.scenario)
            .then(a.mode.to_string().cmp(&b.mode.to_string()))
            .then(a.model.cmp(&b.model))
    });

    stats
}

/// Shorten model name for display.
fn shorten_model_name(model: &str) -> &str {
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

/// Simple timestamp without chrono dependency.
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    format!("{}", duration.as_secs())
}
