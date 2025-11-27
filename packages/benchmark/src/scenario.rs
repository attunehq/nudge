//! Scenarios define test cases for benchmarking rule compliance.
//!
//! Scenarios test along multiple axes:
//! - Agent: which agent are we testing?
//! - Environment: what project is this test run within?
//! - Guidance: what guidance do we give to the agent?
//! - Rule(s): which rule(s) are we testing?
//! - Prompt: what prompt do we give to the agent?
//! - Expectation: what is the final state we want to see in the project?

use std::{
    fmt::{self, Display, Formatter},
    fs::{create_dir_all, read_to_string, write},
    path::{Path, PathBuf},
};

use bon::Builder;
use color_eyre::{
    Result, Section, SectionExt,
    eyre::{Context, bail, eyre},
};
use color_print::cformat;
use glob::glob;
use serde::Deserialize;

use crate::matcher::{MatchString, Matcher};
use crate::outcome::{CommandFailed, ContentMismatch, RegexMatched, RegexNotMatched, Violation};

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

    /// An optional description of what this scenario tests.
    #[builder(into)]
    #[serde(default)]
    pub description: Option<String>,

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

    /// The guidance content to provide to the agent.
    ///
    /// This should contain the content for the agent's context file (e.g.,
    /// `CLAUDE.md` for Claude Code). Whether this guidance is actually applied
    /// depends on the runtime `Guidance` mode passed to `evaluate()`.
    ///
    /// **Important:** Always use this field for guidance content rather than
    /// setup commands, so that evaluators can choose to exclude guidance at
    /// runtime to test agent behavior with and without guidance.
    #[builder(into)]
    pub guidance: String,

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
    /// Evaluate this command and return any violations.
    ///
    /// Returns `Ok(vec![])` if the evaluation passes, `Ok(violations)` if it fails
    /// with expectation violations, or `Err` if there's a fatal error (e.g., file not found).
    #[tracing::instrument]
    pub fn evaluate(&self, project: &Path) -> Result<Vec<Violation>> {
        match self {
            EvaluationCommand::Command(command) => command.evaluate(project),
            EvaluationCommand::Equals(file) => file.equals(project),
            EvaluationCommand::Contains(file) => file.contains(project),
            EvaluationCommand::NotContains(file) => file.not_contains(project),
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
    /// Run the command and return () on success, or an error on failure.
    /// Used for setup commands where failure is fatal.
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

    /// Evaluate the command and return violations if it fails.
    /// Used for evaluation commands where failure is a violation, not a fatal error.
    //
    // Note: this returns a vec of violations so that it has the same API shape
    // as the other evaluation command methods, not because it can actually
    // return multiple violations (although it may in the future).
    #[tracing::instrument]
    pub fn evaluate(&self, project: &Path) -> Result<Vec<Violation>> {
        let output = std::process::Command::new(&self.binary)
            .args(&self.args)
            .current_dir(project)
            .output()
            .with_context(|| format!("run command {:?} with args: {:?}", self.binary, self.args))?;

        if output.status.success() {
            Ok(vec![])
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(vec![Violation::CommandFailed(
                CommandFailed::builder()
                    .command(format!("{} {}", self.binary, self.args.join(" ")))
                    .maybe_exit_code(output.status.code())
                    .stderr(stderr.to_string())
                    .build(),
            )])
        }
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
        if let Some(parent) = path.parent() {
            create_dir_all(parent)
                .with_context(|| format!("create parent directories for {path:?}"))?;
        }
        write(&path, &self.content).with_context(|| format!("write content to {path:?}"))
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
        if let Some(parent) = path.parent() {
            create_dir_all(parent)
                .with_context(|| format!("create parent directories for {path:?}"))?;
        }

        let existing = read_to_string(&path).ok();
        let mut content = existing.unwrap_or_default();

        if !content.is_empty()
            && let Some(separator) = &self.separator
        {
            content.push_str(separator);
        }

        content.push_str(&self.content);
        write(&path, content).with_context(|| format!("write content to {path:?}"))
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
    ///
    /// If the content fails to compile as a regex, the evaluation proceeds
    /// assuming it is a string; in this case the content is tested against the
    /// file contents using string operations.
    #[builder(into)]
    pub content: MatchString,
}

impl FileEvaluation {
    /// Reads all files matching the glob pattern and returns their paths and contents.
    ///
    /// The `path` field supports glob patterns like `**/*.rs` or `src/*.rs`.
    /// If no files match, returns an error.
    #[tracing::instrument(name = "FileEvaluation::read")]
    fn read(&self, project: &Path) -> Result<Vec<(PathBuf, String)>> {
        let path = project.join(&self.path);
        let pattern = path
            .to_str()
            .ok_or_else(|| eyre!("invalid path pattern: {path:?}"))?;

        let files = glob(pattern)
            .with_context(|| format!("parse glob pattern {pattern:?}"))?
            .map(|entry| {
                let path = entry.context("read glob entry")?;
                if path.is_file() {
                    let content = read_to_string(&path).context("read file content")?;
                    Ok(Some((path, content)))
                } else {
                    Ok(None)
                }
            })
            .filter_map(|t| t.transpose())
            .collect::<Result<Vec<_>>>()?;

        if files.is_empty() {
            bail!("no files matched pattern {pattern:?}");
        }

        Ok(files)
    }

    /// Tests that all matched files' contents match the test exactly.
    #[tracing::instrument(name = "FileEvaluation::equals")]
    fn equals(&self, project: &Path) -> Result<Vec<Violation>> {
        let files = self.read(project)?;
        let violations = files
            .iter()
            .filter_map(|(path, content)| {
                if self.content.is_exact_match(content) {
                    None
                } else {
                    let expected = format!("regex: {}", self.content.as_str());
                    Some(Violation::ContentMismatch(
                        ContentMismatch::builder()
                            .path(path)
                            .expected(expected)
                            .build(),
                    ))
                }
            })
            .collect();
        Ok(violations)
    }

    /// Tests that all matched files' contents contain a match for the regex pattern.
    #[tracing::instrument(name = "FileEvaluation::contains")]
    fn contains(&self, project: &Path) -> Result<Vec<Violation>> {
        let files = self.read(project)?;
        let violations = files
            .iter()
            .filter_map(|(path, content)| {
                if self.content.is_match(content) {
                    None
                } else {
                    Some(Violation::RegexNotMatched(
                        RegexNotMatched::builder()
                            .path(path)
                            .pattern(self.content.as_str())
                            .build(),
                    ))
                }
            })
            .collect();
        Ok(violations)
    }

    /// Tests that no matched files' contents contain a match for the regex pattern.
    #[tracing::instrument(name = "FileEvaluation::not_contains")]
    fn not_contains(&self, project: &Path) -> Result<Vec<Violation>> {
        let files = self.read(project)?;
        let violations = files
            .iter()
            .filter_map(|(path, content)| {
                let matches = self.content.find(content);
                matches.spans().next().map(|span| {
                    Violation::RegexMatched(
                        RegexMatched::builder()
                            .path(path)
                            .pattern(self.content.as_str())
                            .source(content)
                            .span(span)
                            .build(),
                    )
                })
            })
            .collect();
        Ok(violations)
    }
}

impl From<&FileEvaluation> for FileEvaluation {
    fn from(value: &FileEvaluation) -> Self {
        value.clone()
    }
}

impl Display for Scenario {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Name
        writeln!(f, "{}", cformat!("<green,bold>name:</>"))?;
        write_indented(f, &self.name, 1)?;
        writeln!(f)?;

        // Description (if present)
        if let Some(description) = &self.description {
            writeln!(f, "{}", cformat!("<green,bold>description:</>"))?;
            write_indented(f, description, 1)?;
            writeln!(f)?;
        }

        // Guidance
        writeln!(f, "{}", cformat!("<green,bold>guidance:</>"))?;
        write_indented(f, &self.guidance, 1)?;
        writeln!(f)?;

        // Prompt
        writeln!(f, "{}", cformat!("<green,bold>prompt:</>"))?;
        write_indented(f, &self.prompt, 1)?;
        writeln!(f)?;

        // Commands
        writeln!(f, "{}", cformat!("<green,bold>commands:</>"))?;
        for command in &self.commands {
            write_setup_command(f, command, 1)?;
        }
        writeln!(f)?;

        // Expected
        writeln!(f, "{}", cformat!("<green,bold>expected:</>"))?;
        for expected in &self.expected {
            write_eval_command(f, expected, 1)?;
        }

        Ok(())
    }
}

impl Display for SetupCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write_setup_command(f, self, 0)
    }
}

impl Display for EvaluationCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write_eval_command(f, self, 0)
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.binary, self.args.join(" "))
    }
}

impl Display for WriteFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)
    }
}

impl Display for AppendFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)
    }
}

impl Display for FileEvaluation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path)
    }
}

// Helper functions for formatted output

fn indent(level: usize) -> String {
    "  ".repeat(level)
}

fn write_indented(f: &mut Formatter<'_>, text: &str, level: usize) -> fmt::Result {
    let prefix = indent(level);
    for line in text.lines() {
        writeln!(f, "{prefix}{line}")?;
    }
    Ok(())
}

fn write_setup_command(f: &mut Formatter<'_>, cmd: &SetupCommand, level: usize) -> fmt::Result {
    let prefix = indent(level);
    match cmd {
        SetupCommand::Command(c) => {
            let cmd_str = c.to_string();
            writeln!(
                f,
                "{prefix}{}",
                cformat!("<cyan>-</> <yellow>command:</> {cmd_str}")
            )
        }
        SetupCommand::Write(file) => {
            let path = &file.path;
            writeln!(
                f,
                "{prefix}{}",
                cformat!("<cyan>-</> <yellow>write:</> {path}")
            )?;
            write_indented(f, &file.content, level + 2)
        }
        SetupCommand::Append(file) => {
            let path = &file.path;
            writeln!(
                f,
                "{prefix}{}",
                cformat!("<cyan>-</> <yellow>append:</> {path}")
            )?;
            if let Some(sep) = &file.separator {
                writeln!(
                    f,
                    "{prefix}  {}",
                    cformat!("<dim>separator:</> <dim>{sep}</>")
                )?;
            }
            write_indented(f, &file.content, level + 2)
        }
    }
}

fn write_eval_command(f: &mut Formatter<'_>, cmd: &EvaluationCommand, level: usize) -> fmt::Result {
    let prefix = indent(level);
    match cmd {
        EvaluationCommand::Command(c) => {
            let cmd_str = c.to_string();
            writeln!(
                f,
                "{prefix}{}",
                cformat!("<cyan>-</> <yellow>command:</> {cmd_str}")
            )
        }
        EvaluationCommand::Equals(file) => write_file_eval(f, &prefix, "equals", file),
        EvaluationCommand::Contains(file) => write_file_eval(f, &prefix, "contains", file),
        EvaluationCommand::NotContains(file) => write_file_eval(f, &prefix, "not_contains", file),
    }
}

fn write_file_eval(
    f: &mut Formatter<'_>,
    prefix: &str,
    kind: &str,
    file: &FileEvaluation,
) -> fmt::Result {
    let kind_label = format!("{kind} (regex):");
    let path = &file.path;
    let content = file.content.to_string();
    writeln!(
        f,
        "{prefix}{}",
        cformat!("<cyan>-</> <yellow>{kind_label}</> {path}")
    )?;
    writeln!(f, "{prefix}  {}", cformat!("<dim>{content}</>"))
}
