//! Terminal UI for displaying Claude's output and prompting the user.

use std::io::{Write, stdin, stdout};

use color_eyre::eyre::{Context, Result};
use color_print::{cformat, cprintln};

use nudge::claude::hook::{PreToolUseEditInput, PreToolUseWebFetchInput, PreToolUseWriteInput};

use super::stream::{ContentBlock, MessageContent, ResultMessage, ToolResultContent};

/// Terminal UI for Claude interactions.
pub struct TerminalUI {
    /// Whether to show verbose output (tool results, etc.)
    verbose: bool,
}

impl TerminalUI {
    /// Create a new terminal UI.
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Display assistant text output.
    pub fn display_text(&self, text: &str) {
        print!("{}", text);
        let _ = stdout().flush();
    }

    /// Display a tool use request.
    pub fn display_tool_use(&self, name: &str, input: &serde_json::Value) {
        // Try to extract a useful summary based on tool type
        let summary = self.tool_summary(name, input);
        if let Some(summary) = summary {
            cprintln!("\n<dim>[{}: {}]</dim>", name, summary);
        } else {
            cprintln!("\n<dim>[{}]</dim>", name);
        }

        if self.verbose
            && let Ok(pretty) = serde_json::to_string_pretty(input)
        {
            cprintln!("<dim>{}</dim>", pretty);
        }
    }

    /// Extract a short summary for known tool types.
    fn tool_summary(&self, name: &str, input: &serde_json::Value) -> Option<String> {
        match name {
            "Read" => {
                let path = input.get("file_path")?.as_str()?;
                Some(path.to_string())
            }
            "Write" => {
                let parsed = serde_json::from_value::<PreToolUseWriteInput>(input.clone()).ok()?;
                Some(parsed.file_path.display().to_string())
            }
            "Edit" => {
                let parsed = serde_json::from_value::<PreToolUseEditInput>(input.clone()).ok()?;
                Some(parsed.file_path.display().to_string())
            }
            "Bash" => {
                let cmd = input.get("command")?.as_str()?;
                // Truncate long commands
                let truncated = if cmd.len() > 60 {
                    format!("{}...", &cmd[..57])
                } else {
                    cmd.to_string()
                };
                Some(format!("`{}`", truncated))
            }
            "Glob" => {
                let pattern = input.get("pattern")?.as_str()?;
                Some(pattern.to_string())
            }
            "Grep" => {
                let pattern = input.get("pattern")?.as_str()?;
                Some(format!("/{}/", pattern))
            }
            "WebFetch" => {
                let parsed =
                    serde_json::from_value::<PreToolUseWebFetchInput>(input.clone()).ok()?;
                Some(parsed.url)
            }
            "WebSearch" => {
                let query = input.get("query")?.as_str()?;
                Some(format!("\"{}\"", query))
            }
            "Task" => {
                let desc = input.get("description")?.as_str()?;
                Some(desc.to_string())
            }
            _ => None,
        }
    }

    /// Display a tool result.
    pub fn display_tool_result(&self, tool_use_id: &str, content: &ToolResultContent) {
        if self.verbose {
            cprintln!("<dim>[Result {}]</dim>", tool_use_id);
            cprintln!("<dim>{}</dim>", content);
        }
    }

    /// Display message content blocks.
    pub fn display_content(&self, content: &MessageContent) {
        for block in content.blocks() {
            match block {
                ContentBlock::Text { text } => self.display_text(&text),
                ContentBlock::ToolUse { id: _, name, input } => {
                    self.display_tool_use(&name, &input);
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                } => {
                    self.display_tool_result(&tool_use_id, &content);
                }
            }
        }
    }

    /// Display session initialization info.
    pub fn display_init(&self, session_id: &str) {
        cprintln!("<dim>Session: {}</dim>", session_id);
    }

    /// Display result/completion info.
    pub fn display_result(&self, result: &ResultMessage) {
        println!(); // Ensure we're on a new line
        if result.is_error {
            cprintln!("<red>Error during conversation</red>");
        }
        if self.verbose {
            if let Some(duration) = result.duration_ms {
                cprintln!("<dim>Duration: {}ms</dim>", duration);
            }
            if let Some(cost) = result.total_cost_usd {
                cprintln!("<dim>Cost: ${:.4}</dim>", cost);
            }
            if let Some(turns) = result.num_turns {
                cprintln!("<dim>Turns: {}</dim>", turns);
            }
        }
    }

    /// Display an error message.
    pub fn display_error(&self, error: &str) {
        cprintln!("<red>Error:</red> {}", error);
    }

    /// Display a status message.
    pub fn display_status(&self, status: &str) {
        cprintln!("<dim>{}</dim>", status);
    }

    /// Prompt the user for input.
    ///
    /// Returns the user's input, or `None` if EOF was reached.
    pub fn prompt(&self) -> Result<Option<String>> {
        print!("\n{}", cformat!("<cyan><bold>You:</bold></cyan> "));
        stdout().flush().wrap_err("Failed to flush stdout")?;

        let mut input = String::new();
        match stdin().read_line(&mut input) {
            Ok(0) => Ok(None), // EOF
            Ok(_) => Ok(Some(input.trim().to_string())),
            Err(e) => Err(e).wrap_err("Failed to read user input"),
        }
    }

    /// Display help for available commands.
    pub fn display_help(&self) {
        cprintln!("<bold>Available commands:</bold>");
        cprintln!("  <cyan>/exit</cyan>, <cyan>/quit</cyan>  - Exit the conversation");
        cprintln!("  <cyan>/help</cyan>        - Show this help");
        cprintln!("  <cyan>/session</cyan>     - Show current session ID");
    }
}
