//! Test rules against sample input.

use std::fs;
use std::path::PathBuf;

use clap::Args;
use color_eyre::eyre::{Context, Result, bail, eyre};
use serde_json::json;

use nudge::{
    agent::claude,
    hook::{NudgeHook, evaluate::evaluate_hooks, response::HookOutcome},
    rules,
};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Name of the rule to test.
    #[arg(long)]
    pub rule: String,

    /// Tool name (for PreToolUse rules): Write, Edit, WebFetch, or Bash.
    #[arg(long)]
    pub tool: Option<String>,

    /// File path (for rules with file patterns).
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// URL to test against (for WebFetch tool).
    #[arg(long)]
    pub url: Option<String>,

    /// Content to test against (for Write tool or Edit new_string).
    #[arg(long, conflicts_with = "content_file")]
    pub content: Option<String>,

    /// Path to a file whose content to test against.
    #[arg(long, conflicts_with = "content")]
    pub content_file: Option<PathBuf>,

    /// User prompt to test against (for UserPromptSubmit rules).
    #[arg(long)]
    pub prompt: Option<String>,
}

pub fn main(config: Config) -> Result<()> {
    let loaded_config = rules::load_all()?;
    let rule = loaded_config
        .rules
        .into_iter()
        .find(|r| r.name == config.rule)
        .ok_or_else(|| eyre!("Rule '{}' not found", config.rule))?;

    let hooks = build_hooks(&config)?;

    println!("Rule: {}", config.rule);
    println!();

    match evaluate_hooks(&hooks, &[rule]) {
        HookOutcome::Passthrough => {
            println!("Result: Passthrough");
            println!("The rule did not match the provided input.");
        }
        HookOutcome::DenyPreToolUse { message } => {
            println!("Result: Interrupt");
            println!();
            println!("Matched content:");
            println!("{message}");
        }
        HookOutcome::UpdatePreToolUse {
            system_message,
            additional_context,
            updated_input,
        } => {
            println!("Result: Substitute");
            println!();
            println!("{system_message}");
            println!("{additional_context}");
            println!("{}", serde_json::to_string_pretty(&updated_input)?);
        }
        HookOutcome::AddContext { context } => {
            println!("Result: Continue");
            println!();
            println!("Matched content:");
            println!("{context}");
        }
        HookOutcome::ContinueStop { reason } => {
            println!("Result: Continue Stop");
            println!();
            println!("{reason}");
        }
    }

    Ok(())
}

/// Build a Hook from the CLI arguments.
fn build_hooks(config: &Config) -> Result<Vec<NudgeHook>> {
    // Determine what kind of hook to build based on arguments
    if config.prompt.is_some() {
        return build_user_prompt_hook(config);
    }

    if config.tool.is_some()
        || config.file.is_some()
        || config.url.is_some()
        || config.content.is_some()
        || config.content_file.is_some()
    {
        return build_tool_use_hook(config);
    }

    bail!(
        "Must specify either --prompt (for UserPromptSubmit) or --tool/--file/--content (for PreToolUse)"
    );
}

fn build_user_prompt_hook(config: &Config) -> Result<Vec<NudgeHook>> {
    let prompt = config
        .prompt
        .as_ref()
        .expect("prompt required for UserPromptSubmit hook");

    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "prompt": prompt
    });

    claude::parse_hook(payload).context("failed to build UserPromptSubmit hook")
}

fn build_tool_use_hook(config: &Config) -> Result<Vec<NudgeHook>> {
    let tool = config.tool.as_deref().unwrap_or("Write");
    let file_path = config
        .file
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "test.txt".to_string());

    // Get content from --content or --content-file
    let content = match (&config.content, &config.content_file) {
        (Some(c), _) => c.clone(),
        (_, Some(path)) => fs::read_to_string(path)
            .with_context(|| format!("failed to read content from {}", path.display()))?,
        _ => String::new(),
    };

    let tool_input = match tool {
        "Write" => json!({
            "file_path": file_path,
            "content": content
        }),
        "Edit" => json!({
            "file_path": file_path,
            "old_string": "",
            "new_string": content
        }),
        "WebFetch" => {
            let url = config
                .url
                .clone()
                .unwrap_or_else(|| "https://example.com".to_string());
            json!({
                "url": url,
                "prompt": content
            })
        }
        "Bash" => json!({
            "command": content,
            "description": "Test command"
        }),
        other => bail!(
            "Unknown tool '{}'. Supported tools: Write, Edit, WebFetch, Bash",
            other
        ),
    };

    let payload = json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "tool_name": tool,
        "tool_use_id": "test-123",
        "tool_input": tool_input
    });

    claude::parse_hook(payload).context("failed to build PreToolUse hook")
}
