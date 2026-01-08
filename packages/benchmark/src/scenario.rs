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
use color_print::{cformat, cwriteln};
use derive_more::Debug;
use glob_match::glob_match;
use serde::Deserialize;
use tap::{Pipe, Tap};
use walkdir::WalkDir;

use crate::{
    ext::indent,
    matcher::FallibleMatcher,
    matcher::code::CodeMatcher,
    outcome::{
        CommandFailed, CommandSucceeded, Evidence, Outcome, QueryMatchedEvidence,
        QueryMatchedViolation, QueryNotMatchedEvidence, QueryNotMatchedViolation, Violation,
    },
};

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

impl Display for Scenario {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let name = &self.name;
        let description = self.description.as_deref().unwrap_or("<no description>");
        let guidance = &self.guidance;
        let prompt = &self.prompt;
        let commands = self
            .commands
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .indent(2);
        let expected = self
            .expected
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .indent(2);
        let metadata = [
            cformat!("<green,bold>name:</> {name}"),
            cformat!("<green,bold>description:</> {description}"),
            cformat!("<green,bold>guidance:</> {guidance}"),
            cformat!("<green,bold>prompt:</> {prompt}"),
            cformat!("<green,bold>commands:</> {commands}"),
            cformat!("<green,bold>expected:</> {expected}"),
        ];
        for meta in metadata {
            writeln!(f, "{meta}")?;
        }
        Ok(())
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

impl Display for SetupCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SetupCommand::Command(command) => {
                let metadata = command.to_string().indent(2);
                cwriteln!(f, "<cyan>-</> <yellow>command:</>")?;
                writeln!(f, "{metadata}")?;
            }
            SetupCommand::Write(file) => {
                let metadata = file.to_string().indent(2);
                cwriteln!(f, "<cyan>-</> <yellow>write:</>")?;
                writeln!(f, "{metadata}")?;
            }
            SetupCommand::Append(file) => {
                let metadata = file.to_string().indent(2);
                cwriteln!(f, "<cyan>-</> <yellow>append:</>")?;
                writeln!(f, "{metadata}")?;
            }
        }
        Ok(())
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
    /// considered failed.
    Command(Command),

    /// Validate there are matches for the specified query.
    Exists(FileEvaluation),

    /// Validate there are no matches for the specified query.
    NotExists(FileEvaluation),
}

impl EvaluationCommand {
    /// Evaluate this command and return an outcome.
    ///
    /// Returns `Ok(Outcome::Pass { evidence })` if the evaluation passes,
    /// `Ok(Outcome::Fail { violations })` if it fails with expectation
    /// violations, or `Err` if there's a fatal error (e.g., file not
    /// found).
    #[tracing::instrument]
    pub fn evaluate(&self, project: &Path) -> Result<Outcome> {
        match self {
            EvaluationCommand::Command(test) => test.evaluate(project),
            EvaluationCommand::Exists(test) => {
                tracing::debug!("evaluating exists");
                let files = test.read(project)?;
                let mut violations = Vec::new();
                let mut evidence = Vec::new();
                let filter_desc = test.between.as_ref().map(|f| f.to_string());

                for (path, content) in files {
                    tracing::debug!(path = ?path.to_string_lossy(), "evaluating file");
                    let matches = test
                        .matcher
                        .find(&content)
                        .context("find matches for query")
                        .with_section(|| test.matcher.query.as_str().to_string().header("Query:"))
                        .with_section(|| test.matcher.language.to_string().header("Language:"))
                        .with_section(|| path.to_string_lossy().to_string().header("Path:"))
                        .with_section(|| content.to_string().header("Content:"))?;

                    let matches = match &test.between {
                        Some(filter) => matches.filter_labeled(|cap| filter.matches(&content, cap)),
                        None => matches,
                    };

                    if matches.is_empty() {
                        violations.push(Violation::QueryNotMatched(
                            QueryNotMatchedViolation::builder()
                                .path(path)
                                .query(test.matcher.query.as_str())
                                .language(test.matcher.language)
                                .content(content)
                                .maybe_filter(filter_desc.clone())
                                .build(),
                        ));
                    } else {
                        evidence.push(Evidence::QueryMatched(
                            QueryMatchedEvidence::builder()
                                .path(path.clone())
                                .query(test.matcher.query.as_str())
                                .language(test.matcher.language)
                                .content(content.clone())
                                .matches(matches)
                                .maybe_filter(filter_desc.clone())
                                .build(),
                        ));
                    }
                }

                if violations.is_empty() {
                    Ok(Outcome::Pass { evidence })
                } else {
                    Ok(Outcome::Fail { violations })
                }
            }
            EvaluationCommand::NotExists(test) => {
                tracing::debug!("evaluating not exists");
                let files = test.read(project)?;
                let mut violations = Vec::new();
                let mut evidence = Vec::new();
                let filter_desc = test.between.as_ref().map(|f| f.to_string());

                for (path, content) in files {
                    tracing::debug!(path = ?path.to_string_lossy(), "evaluating file");
                    let matches = test
                        .matcher
                        .find(&content)
                        .context("find matches for query")
                        .with_section(|| test.matcher.query.as_str().to_string().header("Query:"))
                        .with_section(|| test.matcher.language.to_string().header("Language:"))
                        .with_section(|| path.to_string_lossy().to_string().header("Path:"))
                        .with_section(|| content.to_string().header("Content:"))?;

                    // Apply between filter if present
                    let matches = match &test.between {
                        Some(filter) => {
                            matches.filter_labeled(|captures| filter.matches(&content, captures))
                        }
                        None => matches,
                    };

                    if !matches.is_empty() {
                        violations.push(Violation::QueryMatched(
                            QueryMatchedViolation::builder()
                                .path(path.clone())
                                .query(test.matcher.query.as_str())
                                .language(test.matcher.language)
                                .content(content.clone())
                                .matches(matches)
                                .maybe_filter(filter_desc.clone())
                                .build(),
                        ));
                    } else {
                        evidence.push(Evidence::QueryNotMatched(
                            QueryNotMatchedEvidence::builder()
                                .path(path)
                                .query(test.matcher.query.as_str())
                                .language(test.matcher.language)
                                .content(content)
                                .maybe_filter(filter_desc.clone())
                                .build(),
                        ));
                    }
                }

                if violations.is_empty() {
                    Ok(Outcome::Pass { evidence })
                } else {
                    Ok(Outcome::Fail { violations })
                }
            }
        }
    }
}

impl From<&EvaluationCommand> for EvaluationCommand {
    fn from(value: &EvaluationCommand) -> Self {
        value.clone()
    }
}

impl Display for EvaluationCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EvaluationCommand::Command(command) => {
                let metadata = command.to_string().indent(2);
                cwriteln!(f, "<cyan>-</> <yellow>command:</>")?;
                writeln!(f, "{metadata}")?;
            }
            EvaluationCommand::Exists(query) => {
                let metadata = query.to_string().indent(2);
                cwriteln!(f, "<cyan>-</> <yellow>exists:</>")?;
                writeln!(f, "{metadata}")?;
            }
            EvaluationCommand::NotExists(query) => {
                let metadata = query.to_string().indent(2);
                cwriteln!(f, "<cyan>-</> <yellow>not_exists:</>")?;
                writeln!(f, "{metadata}")?;
            }
        }
        Ok(())
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
        tracing::debug!("running command");
        std::process::Command::new(&self.binary)
            .args(&self.args)
            .current_dir(project)
            .output()
            .with_context(|| format!("run command {:?} with args: {:?}", self.binary, self.args))
            .and_then(|output| {
                if output.status.success() {
                    tracing::debug!("command succeeded");
                    Ok(())
                } else {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::debug!(?stderr, ?stdout, "command failed");
                    Err(eyre!(
                        "run command {:?} with args: {:?}",
                        self.binary,
                        self.args
                    ))
                    .with_section(|| self.binary.clone().header("Command:"))
                    .with_section(|| self.args.join("\n").header("Arguments:"))
                    .section(stdout.to_string().header("Stdout:"))
                    .section(stderr.to_string().header("Stderr:"))
                }
            })
    }

    /// Evaluate the command and return an outcome.
    /// Used for evaluation commands where failure is a violation, not a fatal
    /// error.
    #[tracing::instrument]
    pub fn evaluate(&self, project: &Path) -> Result<Outcome> {
        let output = std::process::Command::new(&self.binary)
            .args(&self.args)
            .current_dir(project)
            .tap(|cmd| tracing::debug!(?cmd, "running evaluation command"))
            .output()
            .with_context(|| "run command".to_string())
            .with_section(|| self.binary.clone().header("Command:"))
            .with_section(|| self.args.join("\n").header("Arguments:"))?;

        let command = format!("{} {}", self.binary, self.args.join(" "));
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        if output.status.success() {
            tracing::debug!("command succeeded");
            CommandSucceeded::builder()
                .command(command)
                .maybe_exit_code(output.status.code())
                .stdout(stdout)
                .stderr(stderr)
                .build()
                .pipe(Evidence::CommandSucceeded)
                .pipe(|e| Outcome::Pass { evidence: vec![e] })
                .pipe(Ok)
        } else {
            tracing::debug!("command failed");
            CommandFailed::builder()
                .command(command)
                .maybe_exit_code(output.status.code())
                .stderr(stderr)
                .stdout(stdout)
                .build()
                .pipe(Violation::CommandFailed)
                .pipe(|v| Outcome::Fail {
                    violations: vec![v],
                })
                .pipe(Ok)
        }
    }
}

impl From<&Command> for Command {
    fn from(value: &Command) -> Self {
        value.clone()
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let binary = &self.binary;
        let args = self.args.join("\n").indent(2);
        cwriteln!(f, "<cyan>-</> <yellow>binary:</> {binary}")?;
        cwriteln!(f, "<cyan>-</> <yellow>arguments:</> {args}")?;
        Ok(())
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
    #[debug("{} bytes", content.len())]
    #[builder(into)]
    pub content: String,
}

impl WriteFile {
    #[tracing::instrument]
    pub fn run(&self, project: &Path) -> Result<()> {
        tracing::debug!("writing file");
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

impl Display for WriteFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let path = &self.path;
        let content = self.content.clone().indent(2);
        cwriteln!(f, "<cyan>-</> <yellow>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <yellow>content:</>")?;
        cwriteln!(f, "<dim>{content}</>")?;
        Ok(())
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
    #[debug("{} bytes", content.len())]
    #[builder(into)]
    pub content: String,

    /// An optional string to insert between the existing content and the
    /// new content. If not specified, no separator is inserted; if the file
    /// did not exist then the content is written without the separator.
    #[debug("{} bytes", separator.as_deref().map(|s| s.len()).unwrap_or(0))]
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

impl Display for AppendFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let path = &self.path;
        let content = self.content.clone().indent(2);
        let separator = self.separator.as_deref().unwrap_or("<none>").indent(2);
        cwriteln!(f, "<cyan>-</> <yellow>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <yellow>separator:</>")?;
        cwriteln!(f, "<dim>{separator}</>")?;
        cwriteln!(f, "<cyan>-</> <yellow>content:</>")?;
        cwriteln!(f, "<dim>{content}</>")?;
        Ok(())
    }
}

/// A filter that checks text between two captures.
///
/// This allows validation of whitespace, comments, or other content that
/// exists between AST nodes but isn't captured by tree-sitter queries.
#[derive(Debug, Clone, Deserialize, Builder)]
#[non_exhaustive]
pub struct BetweenFilter {
    /// The label of the capture that starts the region.
    ///
    /// The validated text starts at the end of this capture.
    #[builder(into)]
    pub from: String,

    /// The label of the capture that ends the region.
    ///
    /// The validated text ends at the start of this capture.
    #[builder(into)]
    pub to: String,

    /// If set, only matches where the between-text contains this string pass.
    #[builder(into)]
    #[serde(default)]
    pub contains: Option<String>,

    /// If set, only matches where the between-text does NOT contain this string
    /// pass.
    #[builder(into)]
    #[serde(default)]
    pub not_contains: Option<String>,
}

impl BetweenFilter {
    /// Check if a match passes this filter.
    ///
    /// Returns `true` if the match passes, `false` if it should be filtered
    /// out.
    pub fn matches(&self, source: &str, captures: &[crate::matcher::LabeledSpan]) -> bool {
        let Some(between) = self.extract_between(source, captures) else {
            return false;
        };

        let contains_ok = self
            .contains
            .as_ref()
            .map(|c| between.contains(c.as_str()))
            .unwrap_or(true);

        let not_contains_ok = self
            .not_contains
            .as_ref()
            .map(|c| !between.contains(c.as_str()))
            .unwrap_or(true);

        contains_ok && not_contains_ok
    }

    /// Extract the text between the two captures.
    fn extract_between<'a>(
        &self,
        source: &'a str,
        captures: &[crate::matcher::LabeledSpan],
    ) -> Option<&'a str> {
        let from = captures.iter().find(|c| c.label == self.from)?;
        let to = captures.iter().find(|c| c.label == self.to)?;

        let start = from.end();
        let end = to.start();

        if start <= end {
            Some(&source[start..end])
        } else {
            None
        }
    }
}

impl Display for BetweenFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let from = &self.from;
        let to = &self.to;
        cwriteln!(f, "<cyan>-</> <yellow>from:</> {from}")?;
        cwriteln!(f, "<cyan>-</> <yellow>to:</> {to}")?;
        if let Some(contains) = &self.contains {
            cwriteln!(f, "<cyan>-</> <yellow>contains:</> {contains:?}")?;
        }
        if let Some(not_contains) = &self.not_contains {
            cwriteln!(f, "<cyan>-</> <yellow>not_contains:</> {not_contains:?}")?;
        }
        Ok(())
    }
}

/// A file to evaluate and the content with which to evaluate it.
#[derive(Debug, Clone, Deserialize, Builder)]
#[non_exhaustive]
pub struct FileEvaluation {
    /// The path to the file(s) to evaluate, relative to the environment root.
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

    /// The matcher used to select content out of evaluation files to evaluate.
    #[builder(into)]
    pub matcher: CodeMatcher,

    /// Optional filter to apply to matches based on text between captures.
    ///
    /// When set, matches are filtered based on the text between two named
    /// captures. This allows validation of content that exists between AST
    /// nodes but isn't directly queryable via tree-sitter (e.g., whitespace,
    /// blank lines, comments).
    #[builder(into)]
    #[serde(default)]
    pub between: Option<BetweenFilter>,
}

impl FileEvaluation {
    /// Reads all files matching the glob pattern and returns their paths and
    /// contents.
    ///
    /// The `path` field supports glob patterns like `**/*.rs` or `src/*.rs`.
    /// If no files match, returns an error.
    #[tracing::instrument(name = "FileEvaluation::read")]
    fn read(&self, project: &Path) -> Result<Vec<(PathBuf, String)>> {
        let path = project.join(&self.path);
        let pattern = path
            .to_str()
            .ok_or_else(|| eyre!("invalid path pattern: {path:?}"))?;

        let files = WalkDir::new(project)
            .into_iter()
            .map(|entry| -> Result<_> {
                let entry = entry.context("read walkdir entry")?;

                if entry.file_type().is_file() {
                    let path = entry.path();
                    let relative = path.strip_prefix(project).map_err(|error| eyre!("file is not inside project: {error:?}"))?;
                    tracing::debug!(path = ?path.to_string_lossy(), relative = ?relative.to_string_lossy(), glob = ?self.path, "walk entry");

                    if glob_match(&self.path, relative.to_string_lossy().as_ref()) {
                        tracing::debug!("file matches glob");
                        let content = read_to_string(path).context("read file content")?;
                        Ok(Some((path.to_path_buf(), content)))
                    } else {
                        tracing::debug!("file does not match glob");
                        Ok(None)
                    }
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
}

impl From<&FileEvaluation> for FileEvaluation {
    fn from(value: &FileEvaluation) -> Self {
        value.clone()
    }
}

impl Display for FileEvaluation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let path = &self.path;
        let language = self.matcher.language;
        let query = self.matcher.query.as_str();
        cwriteln!(f, "<cyan>-</> <yellow>path:</> {path}")?;
        cwriteln!(f, "<cyan>-</> <yellow>language:</> {language}")?;
        cwriteln!(f, "<cyan>-</> <yellow>query:</> {query}")?;
        if let Some(between) = &self.between {
            cwriteln!(f, "<cyan>-</> <yellow>between:</>")?;
            write!(f, "{}", between.to_string().indent(2))?;
        }
        Ok(())
    }
}
