//! Run Claude Code with Nudge as the frontend.

use std::path::PathBuf;

use clap::Args;
use color_eyre::eyre::{Context, Result};
use tracing::{debug, instrument};

mod process;
mod stream;
mod ui;

use process::{ClaudeProcess, SpawnOptions};
use stream::{InputMessage, OutputMessage};
use ui::TerminalUI;

/// Configuration for the run command.
#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Initial prompt to send to Claude.
    pub prompt: Option<String>,

    /// Continue the most recent conversation.
    #[arg(short, long)]
    pub r#continue: bool,

    /// Resume a specific session by ID.
    #[arg(short, long)]
    pub resume: Option<String>,

    /// Maximum number of agentic turns.
    #[arg(long)]
    pub max_turns: Option<u32>,

    /// Model to use.
    #[arg(long)]
    pub model: Option<String>,

    /// Show verbose output (tool inputs/outputs).
    #[arg(short, long)]
    pub verbose: bool,

    /// Working directory (defaults to current directory).
    #[arg(long)]
    pub cwd: Option<PathBuf>,
}

impl From<&Config> for SpawnOptions {
    fn from(config: &Config) -> Self {
        Self {
            prompt: config.prompt.clone(),
            continue_session: config.r#continue,
            resume: config.resume.clone(),
            max_turns: config.max_turns,
            model: config.model.clone(),
            cwd: config.cwd.clone(),
        }
    }
}

/// Main entry point for the run command.
#[instrument(skip_all)]
pub fn main(config: Config) -> Result<()> {
    let ui = TerminalUI::new(config.verbose);

    // If no prompt provided, prompt the user first
    let config = if config.prompt.is_none() && !config.r#continue && config.resume.is_none() {
        let mut config = config;
        ui.display_status("Enter your prompt (or /help for commands):");
        match ui.prompt()? {
            Some(input) if input.is_empty() => {
                ui.display_error("No prompt provided");
                return Ok(());
            }
            Some(input) if input.starts_with('/') => {
                handle_command(&input, None, &ui)?;
                return Ok(());
            }
            Some(input) => {
                config.prompt = Some(input);
                config
            }
            None => {
                // EOF
                return Ok(());
            }
        }
    } else {
        config
    };

    let opts = SpawnOptions::from(&config);
    debug!(?opts, "Spawning Claude with options");

    let mut process = ClaudeProcess::spawn(opts).wrap_err("Failed to start Claude")?;

    run_loop(&mut process, &ui)
}

/// Main conversation loop.
fn run_loop(process: &mut ClaudeProcess, ui: &TerminalUI) -> Result<()> {
    loop {
        // Read messages until we get a result or EOF
        let should_prompt = read_until_result(process, ui)?;

        if !should_prompt {
            // Process ended
            break;
        }

        // Prompt for next input
        match ui.prompt()? {
            None => {
                // EOF
                ui.display_status("Goodbye!");
                break;
            }
            Some(input) if input.is_empty() => {
                // Empty input, prompt again
                continue;
            }
            Some(input) if input.starts_with('/') => {
                // Command
                match handle_command(&input, Some(process), ui)? {
                    LoopAction::Continue => continue,
                    LoopAction::Exit => break,
                }
            }
            Some(input) => {
                // Send user message
                let msg = InputMessage::user(&input);
                process
                    .send_message(&msg)
                    .wrap_err("Failed to send message to Claude")?;
            }
        }
    }

    Ok(())
}

/// Read messages from Claude until we get a result message or EOF.
///
/// Returns `true` if we should prompt for more input, `false` if the conversation ended.
fn read_until_result(process: &mut ClaudeProcess, ui: &TerminalUI) -> Result<bool> {
    loop {
        match process.read_message()? {
            None => {
                // EOF - process ended
                return Ok(false);
            }
            Some(OutputMessage::System(sys)) => {
                if sys.subtype == "init" {
                    if let Some(ref session_id) = sys.session_id {
                        ui.display_init(session_id);
                    }
                }
            }
            Some(OutputMessage::Assistant(asst)) => {
                ui.display_content(&asst.message.content);
            }
            Some(OutputMessage::User(usr)) => {
                // Tool results - display if verbose
                ui.display_content(&usr.message.content);
            }
            Some(OutputMessage::Result(res)) => {
                ui.display_result(&res);
                if res.is_error {
                    return Ok(false);
                }
                return Ok(true);
            }
        }
    }
}

/// Action to take after handling a command.
enum LoopAction {
    Continue,
    Exit,
}

/// Handle a slash command.
fn handle_command(
    input: &str,
    process: Option<&ClaudeProcess>,
    ui: &TerminalUI,
) -> Result<LoopAction> {
    let cmd = input.trim_start_matches('/').to_lowercase();
    let cmd = cmd.split_whitespace().next().unwrap_or("");

    match cmd {
        "exit" | "quit" | "q" => {
            ui.display_status("Goodbye!");
            Ok(LoopAction::Exit)
        }
        "help" | "h" | "?" => {
            ui.display_help();
            Ok(LoopAction::Continue)
        }
        "session" => {
            if let Some(process) = process {
                if let Some(session_id) = process.session_id() {
                    ui.display_status(&format!("Session ID: {}", session_id));
                } else {
                    ui.display_status("No session ID yet");
                }
            } else {
                ui.display_status("No active session");
            }
            Ok(LoopAction::Continue)
        }
        _ => {
            ui.display_error(&format!("Unknown command: /{}", cmd));
            ui.display_help();
            Ok(LoopAction::Continue)
        }
    }
}
