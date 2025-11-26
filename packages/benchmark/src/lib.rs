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

use color_eyre::Result;
use color_eyre::eyre::Context;
use tempfile::tempdir;
use tracing::debug;

pub use crate::agent::{Agent, ModelClaudeCode};
pub use crate::scenario::{Guidance, Scenario};

pub mod agent;
pub mod scenario;

#[tracing::instrument(
    skip(scenario),
    fields(
        scenario.name = ?scenario.name,
        scenario.agent = ?scenario.agent,
        project
    )
)]
pub fn evaluate(scenario: &Scenario) -> Result<()> {
    let project = tempdir().context("create temporary project directory")?;
    let root = project.path();
    tracing::Span::current().record("project", format!("{root:?}"));

    for command in &scenario.commands {
        debug!(scenario.command = ?command, "running setup command");
        command
            .run(root)
            .with_context(|| format!("run command {command:?} in {root:?}"))?;
    }

    debug!(?scenario.guidance, "configuring agent guidance");
    match &scenario.guidance {
        Guidance::None => Ok(()),
        Guidance::Pavlov => scenario.agent.configure_pavlov(root),
        Guidance::File(content) => scenario.agent.write_context(root, content),
    }?;

    debug!(?scenario.prompt, "running agent");
    scenario
        .agent
        .run(root, &scenario.prompt)
        .with_context(|| format!("run agent on {root:?}"))?;

    for command in &scenario.expected {
        debug!(scenario.expected = ?command, "running expectation");
        command
            .run(root)
            .with_context(|| format!("evaluate expectation {command:?} in {root:?}"))?;
    }

    Ok(())
}
