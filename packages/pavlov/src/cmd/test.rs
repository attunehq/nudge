//! Test rules against sample input.

use std::path::PathBuf;

use clap::Args;
use color_eyre::eyre::{bail, Context, Result};
use serde_json::json;

use pavlov::claude::hook::Hook;
use pavlov::rules::config::load_all_rules;
use pavlov::rules::eval::CompiledRule;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Name of the rule to test.
    #[arg(long)]
    pub rule: String,

    /// Tool name (for PreToolUse/PostToolUse rules).
    #[arg(long)]
    pub tool: Option<String>,

    /// File path (for rules with file patterns).
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Content to test against (for Write tool).
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
    // Load all rules and find the one we want
    let rules = load_all_rules()?;
    let rule = rules
        .into_iter()
        .find(|r| r.name == config.rule)
        .ok_or_else(|| color_eyre::eyre::eyre!("Rule '{}' not found", config.rule))?;

    // Compile the rule
    let compiled = CompiledRule::compile(rule.clone())
        .with_context(|| format!("failed to compile rule '{}'", config.rule))?;

    // Build a hook payload based on the provided arguments
    let hook = build_hook(&config)?;

    // Evaluate the rule
    let result = compiled.evaluate(&hook);

    // Print results
    println!("Rule: {}", config.rule);

    match result {
        Some(res) => {
            let action_str = if res.is_interrupt { "INTERRUPT" } else { "CONTINUE" };
            println!("Result: {}", action_str);
            println!("Message:");
            for line in res.message.lines() {
                println!("  {}", line);
            }
        }
        None => {
            println!("Result: NO MATCH");
            println!("The rule did not match the provided input.");
        }
    }

    Ok(())
}

/// Build a Hook from the CLI arguments.
fn build_hook(config: &Config) -> Result<Hook> {
    // Determine what kind of hook to build based on arguments
    if config.prompt.is_some() {
        return build_user_prompt_hook(config);
    }

    if config.tool.is_some() || config.file.is_some() || config.content.is_some() || config.content_file.is_some() {
        return build_tool_use_hook(config);
    }

    bail!("Must specify either --prompt (for UserPromptSubmit) or --tool/--file/--content (for PreToolUse)");
}

fn build_user_prompt_hook(config: &Config) -> Result<Hook> {
    let prompt = config.prompt.as_ref().unwrap();

    let payload = json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "prompt": prompt
    });

    let hook: Hook = serde_json::from_value(payload)
        .context("failed to build UserPromptSubmit hook")?;

    Ok(hook)
}

fn build_tool_use_hook(config: &Config) -> Result<Hook> {
    let tool = config.tool.as_deref().unwrap_or("Write");
    let file_path = config.file.as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "test.txt".to_string());

    // Get content from --content or --content-file
    let content = match (&config.content, &config.content_file) {
        (Some(c), _) => c.clone(),
        (_, Some(path)) => std::fs::read_to_string(path)
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
        _ => json!({
            "file_path": file_path
        }),
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

    let hook: Hook = serde_json::from_value(payload)
        .context("failed to build PreToolUse hook")?;

    Ok(hook)
}
