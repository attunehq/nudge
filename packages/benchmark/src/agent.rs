//! Agents that are configured to be evaluated by the benchmark.

use std::path::Path;

use color_eyre::{
    Result, Section, SectionExt,
    eyre::{Context, eyre},
};
use serde::{Deserialize, Serialize};
use serde_plain::{derive_display_from_serialize, derive_fromstr_from_deserialize};

/// Specifies the agent being evaluated.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "model")]
pub enum Agent {
    /// Claude Code: https://www.claude.com/product/claude-code
    ClaudeCode(ModelClaudeCode),
}

impl Agent {
    /// Executes the agent with the given prompt, returning its output.
    #[tracing::instrument]
    pub fn run(&self, project: &Path, prompt: &str) -> Result<()> {
        match self {
            Agent::ClaudeCode(model) => std::process::Command::new("claude")
                .arg("--model")
                .arg(model.to_string())
                .arg("--print")
                .arg(prompt)
                .current_dir(project)
                .output()
                .with_context(|| format!("run {self:?} with prompt {prompt}"))
                .and_then(|output| {
                    if output.status.success() {
                        Ok(())
                    } else {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        Err(eyre!("run {self:?} with prompt {prompt}"))
                            .section(stdout.to_string().header("Stdout:"))
                            .section(stderr.to_string().header("Stderr:"))
                    }
                }),
        }
    }

    /// Write a context file for the agent to the given project directory.
    ///
    /// What specific context file is written depends on the agent type. For
    /// example the Claude Code agent will write a `CLAUDE.md` file.
    #[tracing::instrument]
    pub fn write_context(&self, project: &Path, context: &str) -> Result<()> {
        match self {
            Agent::ClaudeCode(_) => {
                let target = project.join("CLAUDE.md");
                std::fs::write(&target, context)
                    .with_context(|| format!("write context to {target:?}"))
            }
        }
    }

    /// Configure Pavlov for the agent.
    #[tracing::instrument]
    pub fn configure_pavlov(&self, project: &Path) -> Result<()> {
        match self {
            Agent::ClaudeCode(_) => std::process::Command::new("pavlov")
                .arg("claude")
                .arg("setup")
                .current_dir(project)
                .output()
                .with_context(|| format!("run Pavlov setup for {self:?}"))
                .and_then(|output| {
                    if output.status.success() {
                        Ok(())
                    } else {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        Err(eyre!("run Pavlov setup"))
                            .section(stdout.to_string().header("Stdout:"))
                            .section(stderr.to_string().header("Stderr:"))
                    }
                }),
        }
    }
}

/// Specifies the model to use for the Claude Code agent.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ModelClaudeCode {
    /// Alias for the latest Sonnet model.
    #[serde(rename = "sonnet")]
    SonnetLatest,

    /// Alias for the latest Haiku model.
    #[serde(rename = "haiku")]
    HaikuLatest,

    /// Alias for the latest Opus model.
    #[serde(rename = "opus")]
    OpusLatest,

    /// A custom full name for a model, e.g. "claude-sonnet-4-5-20250929"
    Custom(String),
}

derive_fromstr_from_deserialize!(ModelClaudeCode);
derive_display_from_serialize!(ModelClaudeCode);
