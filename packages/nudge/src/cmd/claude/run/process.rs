//! Subprocess management for Claude Code.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use color_eyre::eyre::{Context, Result};
use tracing::{debug, trace};

use super::stream::{InputMessage, OutputMessage};

/// A running Claude Code process.
pub struct ClaudeProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    session_id: Option<String>,
}

/// Options for spawning a Claude process.
#[derive(Debug, Default)]
pub struct SpawnOptions {
    /// Initial prompt to send
    pub prompt: Option<String>,

    /// Resume the most recent session
    pub continue_session: bool,

    /// Resume a specific session by ID
    pub resume: Option<String>,

    /// Maximum number of agentic turns
    pub max_turns: Option<u32>,

    /// Model to use
    pub model: Option<String>,

    /// Working directory
    pub cwd: Option<std::path::PathBuf>,
}

impl ClaudeProcess {
    /// Spawn a new Claude Code process.
    ///
    /// If `opts.prompt` is provided, it will be sent as the first message via
    /// stdin.
    pub fn spawn(opts: SpawnOptions) -> Result<Self> {
        let mut cmd = Command::new("claude");

        // Required flags for JSON I/O
        cmd.arg("-p"); // Non-interactive mode
        cmd.args(["--output-format", "stream-json"]);
        cmd.args(["--input-format", "stream-json"]);
        cmd.arg("--verbose");

        // Session management
        if opts.continue_session {
            cmd.arg("--continue");
        }
        if let Some(ref session_id) = opts.resume {
            cmd.args(["--resume", session_id]);
        }

        // Optional flags
        if let Some(max_turns) = opts.max_turns {
            cmd.args(["--max-turns", &max_turns.to_string()]);
        }
        if let Some(ref model) = opts.model {
            cmd.args(["--model", model]);
        }

        // Working directory
        if let Some(ref cwd) = opts.cwd {
            cmd.current_dir(cwd);
        }

        // Set up pipes
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit()); // Let stderr pass through for debugging

        debug!(?cmd, "Spawning Claude process");

        let mut child = cmd.spawn().wrap_err("Failed to spawn claude process")?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| color_eyre::eyre::eyre!("Failed to capture stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| color_eyre::eyre::eyre!("Failed to capture stdout"))?;

        let mut process = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            session_id: None,
        };

        // Send initial prompt via stdin if provided
        if let Some(ref prompt) = opts.prompt {
            let msg = super::stream::InputMessage::user(prompt);
            process.send_message(&msg)?;
        }

        Ok(process)
    }

    /// Get the session ID if known.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Set the session ID (called when we receive it from init message).
    #[allow(dead_code)]
    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = Some(session_id);
    }

    /// Send a message to Claude via stdin.
    pub fn send_message(&mut self, msg: &InputMessage) -> Result<()> {
        let json = serde_json::to_string(msg).wrap_err("Failed to serialize input message")?;
        trace!(%json, "Sending message to Claude");

        writeln!(self.stdin, "{}", json).wrap_err("Failed to write to Claude stdin")?;
        self.stdin
            .flush()
            .wrap_err("Failed to flush Claude stdin")?;

        Ok(())
    }

    /// Read the next message from Claude's stdout.
    ///
    /// Returns `None` if the process has closed stdout.
    pub fn read_message(&mut self) -> Result<Option<OutputMessage>> {
        let mut line = String::new();

        match self.stdout.read_line(&mut line) {
            Ok(0) => {
                // EOF - process closed stdout
                debug!("Claude process closed stdout");
                return Ok(None);
            }
            Ok(_) => {}
            Err(e) => {
                return Err(e).wrap_err("Failed to read from Claude stdout");
            }
        }

        let line = line.trim();
        if line.is_empty() {
            // Empty line, try again
            return self.read_message();
        }

        trace!(%line, "Received message from Claude");

        let msg = serde_json::from_str::<OutputMessage>(line)
            .wrap_err_with(|| format!("Failed to parse message: {}", line))?;

        // Track session ID
        if let OutputMessage::System(ref sys) = msg
            && let Some(ref session_id) = sys.session_id
        {
            self.session_id = Some(session_id.clone());
        }
        if let OutputMessage::Result(ref res) = msg
            && let Some(ref session_id) = res.session_id
        {
            self.session_id = Some(session_id.clone());
        }

        Ok(Some(msg))
    }

    /// Check if the process is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Wait for the process to exit and return its exit status.
    #[allow(dead_code)]
    pub fn wait(&mut self) -> Result<std::process::ExitStatus> {
        self.child
            .wait()
            .wrap_err("Failed to wait for Claude process")
    }

    /// Kill the process.
    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().wrap_err("Failed to kill Claude process")
    }
}

impl Drop for ClaudeProcess {
    fn drop(&mut self) {
        // Try to kill the process if it's still running
        if self.is_running() {
            let _ = self.kill();
        }
    }
}
