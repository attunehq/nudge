//! Benchmark harness for testing Pavlov rule effectiveness.
//!
//! Scenarios test along multiple axes:
//! - Agent: which agent are we testing?
//! - Environment: what project is this test run within?
//! - Guidance: what guidance do we give to the agent?
//! - Rule(s): which rule(s) are we testing?
//! - Prompt: what prompt do we give to the agent?
//! - Expectation: what is the final state we want to see in the project?
//!
//! Agents are evaluated based on:
//! - How closely do they create the expected final state?
//! - How quickly (wall-clock time) do they complete the task?
//! - How many tokens do they use completing the task?
//!
//! This evaluation benchmark is fundamentally different from existing
//! benchmarks like `SWE-Bench` and friends, because it focuses on _the manner
//! in which the task is completed_ as much as it focuses on whether the task
//! was completed at all.
//!
//! The core thesis here is that:
//! - Most serious software development workflows have specific guidelines
//!   and/or specific implementation methods that they need to use.
//! - Current SOTA coding agents have an inherent issue following specific
//!   guidelines, because guidance competes with general context when performing
//!   the task.
//! - Pavlov addresses this issue by encoding guidance outside of the standard
//!   context loop, in effect providing a learning layer on top of the agent.
//!
//! The scenarios in this benchmark are designed to evaluate the effectiveness
//! of the Pavlov approach compared to other approaches.

use std::{fs::read_to_string, path::Path};

use color_eyre::Result;
use color_eyre::eyre::Context;
use tempfile::tempdir;

pub use crate::agent::{Agent, Guidance, ModelClaudeCode};
pub use crate::outcome::{Outcome, Violation};
pub use crate::scenario::Scenario;

pub mod agent;
pub mod ext;
pub mod matcher;
pub mod outcome;
pub mod scenario;
pub mod snippet;

/// Load all scenarios from the given directory.
///
/// Scenarios are loaded from `.toml` files in the directory and sorted by name.
pub fn load_scenarios(dir: &Path) -> Result<Vec<Scenario>> {
    let mut scenarios = Vec::new();

    let entries = dir
        .read_dir()
        .with_context(|| format!("read scenarios directory: {dir:?}"))?;

    for entry in entries {
        let entry = entry.context("read directory entry")?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "toml") {
            let content =
                read_to_string(&path).with_context(|| format!("read scenario file: {path:?}"))?;
            let scenario = toml::from_str::<Scenario>(&content)
                .with_context(|| format!("parse scenario file: {path:?}"))?;
            scenarios.push(scenario);
        }
    }

    scenarios.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(scenarios)
}

#[tracing::instrument(skip(scenario), fields(scenario = %scenario.name))]
pub fn evaluate(scenario: &Scenario, agent: &Agent, guidance: Guidance) -> Result<Outcome> {
    let project = tempdir().context("create temporary project directory")?;
    let root = project.path();

    for command in &scenario.commands {
        command.run(root)?;
    }

    match guidance {
        Guidance::None => Ok(()),
        Guidance::Pavlov => agent.configure_pavlov(root),
        Guidance::File => agent.write_context(root, &scenario.guidance),
    }?;

    agent.run(root, &scenario.prompt)?;

    let violations =
        scenario
            .expected
            .iter()
            .try_fold(Vec::new(), |mut acc, command| -> Result<_> {
                acc.extend(command.evaluate(root)?);
                Ok(acc)
            })?;

    if violations.is_empty() {
        Ok(Outcome::pass())
    } else {
        Ok(Outcome::fail(violations))
    }
}
