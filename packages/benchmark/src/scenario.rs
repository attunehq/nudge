//! Scenarios define test cases for benchmarking rule compliance.
//!
//! Scenarios test along multiple axes:
//! - Agent: which agent are we testing?
//! - Environment: what project is this test run within?
//! - Guidance: what guidance do we give to the agent?
//! - Rule(s): which rule(s) are we testing?
//! - Prompt: what prompt do we give to the agent?
//! - Expectation: what is the final state we want to see in the project?

use std::path::Path;

use bon::Builder;
use color_eyre::{
    Result, Section, SectionExt,
    eyre::{Context, eyre},
};
use serde::Deserialize;

use crate::agent::Agent;

/// A benchmark scenario testing a specific rule.
///
/// ## Lifecycle
///
/// 1. The scenario is loaded from a file or created in code.
/// 2. The environment is created in a temporary directory using the setup
///    commands specified in the scenario.
/// 3. The guidance is applied to the environment according to the type of
///    guidance specified in the scenario.
/// 4. The prompt is given to the agent and the agent is allowed to run to
///    completion.
/// 5. The expected final state is evaluated according to the evaluation
///    commands specified in the scenario.
/// 6. The scenario is considered passed or failed based on the evaluation
///    commands.
#[derive(Debug, Clone, Deserialize, Builder)]
#[non_exhaustive]
pub struct Scenario {
    /// The name of the scenario.
    #[builder(into)]
    pub name: String,

    /// The agent being evaluated.
    #[builder(into)]
    pub agent: Agent,

    /// The commands to run to set up the environment.
    ///
    /// Environments are always created in a temporary directory, and are
    /// created on demand when the scenario is run.
    ///
    /// This is expected to be something like:
    /// ```toml
    /// commands = [
    ///     { type = "command", binary = "cargo", args = ["init"] },
    ///     { type = "write", path = "src/main.rs", content = "fn main() {}" },
    /// ]
    /// ```
    ///
    /// Or like:
    /// ```toml
    /// commands = [
    ///     { type = "command", binary = "git", args = ["clone", "https://github.com/user/repo.git"] },
    ///     { type = "command", binary = "git", args = ["checkout", "my-eval-branch"] },
    ///     { type = "append", path = "src/main.rs", content = "fn my_new_function() {}", separator = "\n" },
    /// ]
    /// ```
    #[builder(into)]
    pub commands: Vec<SetupCommand>,

    /// The guidance to provide to the agent before running the prompt.
    ///
    /// This is to the agent what `Environment` is to the project: in effect,
    /// this is how we set up the agent itself to be able to run.
    #[builder(into)]
    pub guidance: Guidance,

    /// The prompt to give to the agent.
    #[builder(into)]
    pub prompt: String,

    /// The expected final state of the environment after the agent has run.
    ///
    /// This is expected to be something like:
    /// ```toml
    /// expected = [
    ///     { type = "command", binary = "cargo", args = ["build"] },
    ///     { type = "contains", path = "src/main.rs", content = "fn main() {}" },
    /// ]
    /// ```
    ///
    /// Or like:
    /// ```toml
    /// expected = [
    ///     { type = "command", binary = "cargo", args = ["build"] },
    ///     { type = "not_contains", path = "src/tests/**/*.rs", content = "assert_eq!\\(.+\\)" },
    /// ]
    /// ```
    #[builder(into)]
    pub expected: Vec<EvaluationCommand>,
}

/// The guidance to provide to the agent before running the prompt.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "content", rename_all = "snake_case")]
pub enum Guidance {
    /// Provide no guidance to the agent.
    None,

    /// Set up Pavlov in the environment.
    Pavlov,

    /// Write a context file to the environment.
    ///
    /// The specific implementation of the context file depends on the agent
    /// being evaluated; for example when evaluating `Agent::ClaudeCode` this
    /// will write a `CLAUDE.md` file to the environment with the specified
    /// content.
    File(String),
}

impl From<&Guidance> for Guidance {
    fn from(value: &Guidance) -> Self {
        value.clone()
    }
}

/// A command to run in an environment when it is being created.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "content", rename_all = "snake_case")]
pub enum SetupCommand {
    /// Run the specified command in the environment.
    Command(Command),

    /// Create the specified file in the environment.
    ///
    /// If the file already exists, it is overwritten. If the parent directory
    /// does not exist, it is created before the file is written.
    Write(WriteFile),

    /// Append the specified content to the specified file in the environment.
    ///
    /// If the file already exists, the content is appended to the end of the
    /// file. If not, the file is created with the specified content. If the
    /// parent directory does not exist, it is created before the file is
    /// written.
    Append(AppendFile),
}

impl SetupCommand {
    /// Runs the command in the given project directory.
    #[tracing::instrument]
    pub fn run(&self, project: &Path) -> Result<()> {
        match self {
            SetupCommand::Command(command) => command.run(project).map(drop),
            SetupCommand::Write(file) => file.run(project),
            SetupCommand::Append(file) => file.run(project),
        }
    }
}

impl From<&SetupCommand> for SetupCommand {
    fn from(value: &SetupCommand) -> Self {
        value.clone()
    }
}

/// A command to run in an environment to evaluate the agent's results.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "content", rename_all = "snake_case")]
pub enum EvaluationCommand {
    /// Run the specified command in the environment.
    ///
    /// If the command exits with a non-zero exit code, the scenario is
    /// considered failed; any output from the command is included in the
    /// failure report.
    Command(Command),

    /// Test that the file at the specified path equals the provided content.
    ///
    /// If the file does not exist, the scenario is considered failed.
    /// If the file contents do not match the provided content, the scenario is
    /// considered failed.
    Equals(FileEvaluation),

    /// Test that the file at the specified path contains the provided content.
    ///
    /// If the file does not exist, the scenario is considered failed.
    /// If the file contents do not contain the provided content, the scenario is
    /// considered failed.
    Contains(FileEvaluation),

    /// Test that the file at the specified path does not contain the provided content.
    ///
    /// If the file does not exist, the scenario is considered failed.
    /// If the file contents contain the provided content, the scenario is
    /// considered failed.
    NotContains(FileEvaluation),
}

impl EvaluationCommand {
    #[tracing::instrument]
    pub fn run(&self, project: &Path) -> Result<()> {
        match self {
            EvaluationCommand::Command(command) => command.run(project),
            EvaluationCommand::Equals(file) => file.run_eq(project),
            EvaluationCommand::Contains(file) => file.run_contains(project),
            EvaluationCommand::NotContains(file) => file.run_not_contains(project),
        }
    }
}

impl From<&EvaluationCommand> for EvaluationCommand {
    fn from(value: &EvaluationCommand) -> Self {
        value.clone()
    }
}

/// A command to run.
#[derive(Debug, Clone, Deserialize, Builder)]
#[non_exhaustive]
pub struct Command {
    /// The path to the binary to run, or just the name of the binary if it is
    /// in the user's `$PATH`.
    #[builder(into)]
    pub binary: String,

    /// The arguments to pass to the binary.
    #[builder(into)]
    pub args: Vec<String>,
}

impl Command {
    #[tracing::instrument]
    pub fn run(&self, project: &Path) -> Result<()> {
        std::process::Command::new(&self.binary)
            .args(&self.args)
            .current_dir(project)
            .output()
            .with_context(|| format!("run command {:?} with args: {:?}", self.binary, self.args))
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(eyre!(
                        "run command {:?} with args: {:?}",
                        self.binary,
                        self.args
                    ))
                    .section(stdout.to_string().header("Stdout:"))
                    .section(stderr.to_string().header("Stderr:"))
                }
            })
    }
}

impl From<&Command> for Command {
    fn from(value: &Command) -> Self {
        value.clone()
    }
}

/// Options for writing a file.
#[derive(Debug, Clone, Deserialize, Builder)]
#[non_exhaustive]
pub struct WriteFile {
    /// The path to the file to write, relative to the environment root.
    #[builder(into)]
    pub path: String,

    /// The content to write to the file.
    #[builder(into)]
    pub content: String,
}

impl WriteFile {
    #[tracing::instrument]
    pub fn run(&self, project: &Path) -> Result<()> {
        let path = project.join(&self.path);
        std::fs::write(&path, &self.content).with_context(|| format!("write content to {path:?}"))
    }
}

impl From<&WriteFile> for WriteFile {
    fn from(value: &WriteFile) -> Self {
        value.clone()
    }
}

/// Options for appending a file.
#[derive(Debug, Clone, Deserialize, Builder)]
#[non_exhaustive]
pub struct AppendFile {
    /// The path to the file to append to, relative to the environment root.
    #[builder(into)]
    pub path: String,

    /// The content to append to the file.
    #[builder(into)]
    pub content: String,

    /// An optional string to insert between the existing content and the
    /// new content. If not specified, no separator is inserted; if the file
    /// did not exist then the content is written without the separator.
    #[builder(into)]
    pub separator: Option<String>,
}

impl AppendFile {
    #[tracing::instrument]
    pub fn run(&self, project: &Path) -> Result<()> {
        let path = project.join(&self.path);
        let mut content = std::fs::read_to_string(&path)
            .with_context(|| format!("read content from {path:?}"))?;

        if let Some(separator) = &self.separator {
            content.push_str(separator);
        }

        content.push_str(&self.content);
        std::fs::write(&path, content).with_context(|| format!("append content to {path:?}"))
    }
}

impl From<&AppendFile> for AppendFile {
    fn from(value: &AppendFile) -> Self {
        value.clone()
    }
}

/// A file to evaluate and the content with which to evaluate it.
#[derive(Debug, Clone, Deserialize, Builder)]
#[non_exhaustive]
pub struct FileEvaluation {
    /// The path to the file to evaluate, relative to the environment root.
    ///
    /// This path supports glob patterns, including "globstars":
    /// - `*.rs` matches all `.rs` files in the root directory.
    /// - `**/*.rs` matches all `.rs` files in the environment.
    /// - `src/*.rs` matches all `.rs` files in the `src` directory.
    /// - `src/**/*.rs` matches all `.rs` files in any directory under `src`,
    ///   including `src` itself.
    ///
    /// If this is a glob pattern, all matched files are evaluated; if any files
    /// fail the evaluation then the overall evaluation fails.
    #[builder(into)]
    pub path: String,

    /// The content to evaluate the file against.
    ///
    /// This content supports regular expressions using the RE2 regex syntax
    /// variant supported by the `regex` crate; this content is tested for
    /// whether it "matches" (in the regular expression sense) against the file
    /// contents according to the evaluation operation being performed.
    #[builder(into)]
    pub content: String,
}

impl FileEvaluation {
    #[tracing::instrument]
    fn read(&self, project: &Path) -> Result<String> {
        let path = project.join(&self.path);
        std::fs::read_to_string(&path).with_context(|| format!("read content from {path:?}"))
    }

    #[tracing::instrument]
    fn run_eq(&self, project: &Path) -> Result<()> {
        let content = self.read(project)?;
        if content == self.content {
            Ok(())
        } else {
            Err(eyre!("file content does not match expected content"))
                .section(content.header("File content:"))
                .with_section(|| self.content.clone().header("Expected content:"))
        }
    }

    #[tracing::instrument]
    fn run_contains(&self, project: &Path) -> Result<()> {
        let content = self.read(project)?;
        if content.contains(&self.content) {
            Ok(())
        } else {
            Err(eyre!("file content does not contain expected content"))
                .section(content.header("File content:"))
                .with_section(|| self.content.clone().header("Expected content:"))
        }
    }

    #[tracing::instrument]
    fn run_not_contains(&self, project: &Path) -> Result<()> {
        let content = self.read(project)?;
        if content.contains(&self.content) {
            Err(eyre!("file content contains unexpected content"))
                .section(content.header("File content:"))
                .with_section(|| self.content.clone().header("Unexpected content:"))
        } else {
            Ok(())
        }
    }
}

impl From<&FileEvaluation> for FileEvaluation {
    fn from(value: &FileEvaluation) -> Self {
        value.clone()
    }
}
