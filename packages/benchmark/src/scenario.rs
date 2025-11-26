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
use either::Either;
use glob::glob;
use owo_colors::OwoColorize;
use regex::Regex;
use serde::{Deserialize, Deserializer};
use tap::Pipe;
use tracing::info_span;

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
    #[tracing::instrument]
    pub fn run(&self, project: &Path) -> Result<()> {
        match self {
            EvaluationCommand::Command(command) => command.run(project),
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

        if !content.is_empty() {
            if let Some(separator) = &self.separator {
                content.push_str(separator);
            }
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
    pub content: ContentTest,
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
                tracing::info_span!("FileEvaluation::read::glob_map", ?path).in_scope(|| {
                    if path.is_file() {
                        let content = read_to_string(&path).context("read file content")?;
                        Ok(Some((path, content)))
                    } else {
                        Ok(None)
                    }
                })
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
    fn equals(&self, project: &Path) -> Result<()> {
        let files = self.read(project)?;

        for (path, content) in files {
            info_span!("FileEvaluation::equals::file", ?path, ?content)
                .in_scope(|| self.content.equals(&content))?;
        }

        Ok(())
    }

    /// Tests that all matched files' contents contain a match for the regex pattern.
    #[tracing::instrument(name = "FileEvaluation::contains")]
    fn contains(&self, project: &Path) -> Result<()> {
        let files = self.read(project)?;

        for (path, content) in files {
            info_span!("FileEvaluation::contains::file", ?path, ?content)
                .in_scope(|| self.content.contains(&content))?;
        }

        Ok(())
    }

    /// Tests that no matched files' contents contain a match for the regex pattern.
    #[tracing::instrument(name = "FileEvaluation::not_contains")]
    fn not_contains(&self, project: &Path) -> Result<()> {
        let files = self.read(project)?;

        for (path, content) in files {
            info_span!("FileEvaluation::not_contains::file", ?path, ?content)
                .in_scope(|| self.content.not_contains(&content))?;
        }

        Ok(())
    }
}

impl From<&FileEvaluation> for FileEvaluation {
    fn from(value: &FileEvaluation) -> Self {
        value.clone()
    }
}

/// Test that the content matches a test case, which is either evaluated as a
/// regex or a string.
///
/// If the content is able to compile as regex it is evaluated as a regex.
/// Otherwise it is evaluated as a string.
#[derive(Debug, Clone)]
pub struct ContentTest(Either<Regex, String>);

impl ContentTest {
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        Regex::new(&content)
            .map(Either::Left)
            .unwrap_or_else(|_| Either::Right(content))
            .pipe(Self)
    }

    #[tracing::instrument(name = "ContentTest::equals")]
    fn equals(&self, content: &str) -> Result<()> {
        match &self.0 {
            Either::Left(test) => match test.find(content) {
                Some(bounds) if bounds.start() == 0 && bounds.end() == content.len() => Ok(()),
                Some(_) => bail!("content does not fully match regex"),
                None => bail!("content does not match regex"),
            },
            Either::Right(test) => {
                if content == test {
                    Ok(())
                } else {
                    bail!("content does not match string")
                }
            }
        }
    }

    #[tracing::instrument(name = "ContentTest::contains")]
    fn contains(&self, content: &str) -> Result<()> {
        match &self.0 {
            Either::Left(regex) => {
                if regex.is_match(content) {
                    Ok(())
                } else {
                    Err(eyre!("content does not match regex"))
                        .section(format!("Pattern: {}", regex.as_str()).header("Expected:"))
                }
            }
            Either::Right(needle) => {
                if content.contains(needle) {
                    Ok(())
                } else {
                    Err(eyre!("content does not contain string"))
                        .section(format!("String: {needle:?}").header("Expected:"))
                }
            }
        }
    }

    #[tracing::instrument(name = "ContentTest::not_contains")]
    fn not_contains(&self, content: &str) -> Result<()> {
        match &self.0 {
            Either::Left(regex) => {
                if let Some(m) = regex.find(content) {
                    Err(eyre!("content matches regex"))
                        .section(format!("Pattern: {}", regex.as_str()).header("Regex:"))
                        .section(format!("{:?}", m.as_str()).header("Matched:"))
                } else {
                    Ok(())
                }
            }
            Either::Right(needle) => {
                if content.contains(needle) {
                    Err(eyre!("content contains string"))
                        .section(format!("{needle:?}").header("Found:"))
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl<'de> Deserialize<'de> for ContentTest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let content = String::deserialize(deserializer)?;
        Ok(Self::new(content))
    }
}

impl std::fmt::Display for ContentTest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Either::Left(regex) => write!(f, "{}", regex.as_str()),
            Either::Right(string) => write!(f, "{string}"),
        }
    }
}

impl ContentTest {
    /// Returns whether this test is a regex pattern.
    pub fn is_regex(&self) -> bool {
        self.0.is_left()
    }
}

// ============================================================================
// Display implementations for pretty-printing scenarios
// ============================================================================

impl Display for Scenario {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Name
        writeln!(f, "{}", "name:".green().bold())?;
        write_indented(f, &self.name, 1)?;
        writeln!(f)?;

        // Description (if present)
        if let Some(description) = &self.description {
            writeln!(f, "{}", "description:".green().bold())?;
            write_indented(f, description, 1)?;
            writeln!(f)?;
        }

        // Guidance
        writeln!(f, "{}", "guidance:".green().bold())?;
        write_indented(f, &self.guidance, 1)?;
        writeln!(f)?;

        // Prompt
        writeln!(f, "{}", "prompt:".green().bold())?;
        write_indented(f, &self.prompt, 1)?;
        writeln!(f)?;

        // Commands
        writeln!(f, "{}", "commands:".green().bold())?;
        for command in &self.commands {
            write_setup_command(f, command, 1)?;
        }
        writeln!(f)?;

        // Expected
        writeln!(f, "{}", "expected:".green().bold())?;
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
            writeln!(
                f,
                "{}{} {} {}",
                prefix,
                "-".cyan(),
                "command:".yellow(),
                c.to_string().white()
            )
        }
        SetupCommand::Write(file) => {
            writeln!(
                f,
                "{}{} {} {}",
                prefix,
                "-".cyan(),
                "write:".yellow(),
                file.path.white()
            )?;
            write_indented(f, &file.content, level + 2)
        }
        SetupCommand::Append(file) => {
            writeln!(
                f,
                "{}{} {} {}",
                prefix,
                "-".cyan(),
                "append:".yellow(),
                file.path.white()
            )?;
            if let Some(sep) = &file.separator {
                writeln!(f, "{}  {} {}", prefix, "separator:".dimmed(), sep.dimmed())?;
            }
            write_indented(f, &file.content, level + 2)
        }
    }
}

fn write_eval_command(f: &mut Formatter<'_>, cmd: &EvaluationCommand, level: usize) -> fmt::Result {
    let prefix = indent(level);
    match cmd {
        EvaluationCommand::Command(c) => {
            writeln!(
                f,
                "{}{} {} {}",
                prefix,
                "-".cyan(),
                "command:".yellow(),
                c.to_string().white()
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
    let kind_label = if file.content.is_regex() {
        format!("{kind} (regex):")
    } else {
        format!("{kind}:")
    };
    writeln!(
        f,
        "{}{} {} {}",
        prefix,
        "-".cyan(),
        kind_label.yellow(),
        file.path.white()
    )?;
    writeln!(f, "{}  {}", prefix, file.content.to_string().dimmed())
}
