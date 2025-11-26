//! Agents that are configured to be evaluated by the benchmark.

use std::path::Path;

use color_eyre::{
    Result, Section, SectionExt,
    eyre::{Context, eyre},
};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Specifies the agent being evaluated.
#[derive(Clone, Eq, PartialEq, Debug, Deserialize, Serialize)]
#[serde(tag = "type", content = "model")]
pub enum Agent {
    /// Claude Code: https://www.claude.com/product/claude-code
    ClaudeCode(ModelClaudeCode),
}

impl std::str::FromStr for Agent {
    type Err = String;

    /// Parse an agent from a string like `claude-code:sonnet` or `claude-code:opus`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (agent, model) = s
            .split_once(':')
            .ok_or_else(|| format!("invalid agent format: expected `type:model`, got `{s}`"))?;

        match agent {
            "claude-code" => {
                let model = model
                    .parse::<ModelClaudeCode>()
                    .map_err(|e| format!("invalid model for {agent:?}: {e}"))?;
                Ok(Agent::ClaudeCode(model))
            }
            _ => Err(format!("unknown agent type: {agent:?}")),
        }
    }
}

impl std::fmt::Display for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Agent::ClaudeCode(model) => write!(f, "Claude Code ({model})"),
        }
    }
}

impl Agent {
    /// Returns the human-readable name of the agent type.
    pub fn name(&self) -> &'static str {
        match self {
            Agent::ClaudeCode(_) => "Claude Code",
        }
    }

    /// Returns the model name for this agent.
    pub fn model(&self) -> String {
        match self {
            Agent::ClaudeCode(model) => model.to_string(),
        }
    }
}

impl Agent {
    /// Executes the agent with the given prompt, returning its output.
    #[tracing::instrument]
    pub fn run(&self, project: &Path, prompt: &str) -> Result<()> {
        match self {
            Agent::ClaudeCode(model) => std::process::Command::new("claude")
                .arg("--model")
                .arg(model.as_arg())
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
#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelClaudeCode {
    /// Alias for the latest Sonnet model.
    SonnetLatest,

    /// Alias for the latest Haiku model.
    HaikuLatest,

    /// Alias for the latest Opus model.
    OpusLatest,

    /// A custom full name for a model, e.g. "claude-sonnet-4-5-20250929"
    Custom(String),
}

impl std::str::FromStr for ModelClaudeCode {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "sonnet" => Self::SonnetLatest,
            "haiku" => Self::HaikuLatest,
            "opus" => Self::OpusLatest,
            other => Self::Custom(other.to_string()),
        })
    }
}

impl std::fmt::Display for ModelClaudeCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SonnetLatest => write!(f, "Sonnet"),
            Self::HaikuLatest => write!(f, "Haiku"),
            Self::OpusLatest => write!(f, "Opus"),
            Self::Custom(s) => write!(f, "{s}"),
        }
    }
}

impl ModelClaudeCode {
    /// Returns the CLI argument form of the model name.
    pub fn as_arg(&self) -> &str {
        match self {
            Self::SonnetLatest => "sonnet",
            Self::HaikuLatest => "haiku",
            Self::OpusLatest => "opus",
            Self::Custom(s) => s,
        }
    }
}


/// The type of guidance to provide to the agent before running the prompt.
///
/// This is a runtime choice that determines how the scenario's guidance content
/// is applied (or whether it's applied at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Guidance {
    /// Provide no guidance to the agent.
    #[default]
    None,

    /// Set up Pavlov in the environment.
    Pavlov,

    /// Write the scenario's guidance content to the agent's context file.
    ///
    /// The specific context file depends on the agent type. For example,
    /// `Agent::ClaudeCode` writes to `CLAUDE.md`.
    File,
}

impl std::fmt::Display for Guidance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Pavlov => write!(f, "Pavlov"),
            Self::File => write!(f, "File"),
        }
    }
}
