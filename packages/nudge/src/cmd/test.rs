//! Test rules against sample input.

use std::path::PathBuf;

use clap::Args;
use color_eyre::eyre::{Context, Result, bail, eyre};
use itertools::Itertools;
use serde_json::json;

use nudge::{
    claude::hook::{Hook, PreToolUsePayload},
    rules::{self, Rule},
    snippet::{Match, Source},
};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Name of the rule to test.
    #[arg(long)]
    pub rule: String,

    /// Tool name (for PreToolUse rules): Write, Edit, or WebFetch.
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
    let rules = rules::load_all()?;
    let rule = rules
        .into_iter()
        .find(|r| r.name == config.rule)
        .ok_or_else(|| eyre!("Rule '{}' not found", config.rule))?;

    let hook = build_hook(&config)?;

    println!("Rule: {}", config.rule);
    println!();

    let (matched, source) = evaluate_rule(&rule, &hook);

    if matched.is_empty() {
        println!("Result: Passthrough");
        println!("The rule did not match the provided input.");
    } else {
        let hook_type = match &hook {
            Hook::PreToolUse(_) => "Interrupt",
            Hook::UserPromptSubmit(_) => "Continue",
            Hook::Other => "Passthrough",
        };
        println!("Result: {hook_type}");
        println!();
        println!("Matched content:");
        let annotations = rule.annotate_matches(matched).collect_vec();
        let snippet = source.annotate(annotations);
        println!("{snippet}");
    }

    Ok(())
}

/// Evaluate a single rule against a hook, returning matches and the source.
fn evaluate_rule(rule: &Rule, hook: &Hook) -> (Vec<Match>, Source) {
    match hook {
        Hook::PreToolUse(payload) => match payload {
            PreToolUsePayload::Write(payload) => {
                let matches = rule
                    .hooks_pretooluse_write()
                    .flat_map(|matcher| payload.evaluate(matcher))
                    .collect_vec();
                let source = Source::from(&payload.tool_input.content);
                (matches, source)
            }
            PreToolUsePayload::Edit(payload) => {
                let matches = rule
                    .hooks_pretooluse_edit()
                    .flat_map(|matcher| payload.evaluate(matcher))
                    .collect_vec();
                let source = Source::from(&payload.tool_input.new_string);
                (matches, source)
            }
            PreToolUsePayload::WebFetch(payload) => {
                let matches = rule
                    .hooks_pretooluse_webfetch()
                    .flat_map(|matcher| payload.evaluate(matcher))
                    .collect_vec();
                let source = Source::from(&payload.tool_input.url);
                (matches, source)
            }
            PreToolUsePayload::Other => (Vec::new(), Source::from("")),
        },
        Hook::UserPromptSubmit(payload) => {
            let matches = rule
                .hooks_userpromptsubmit()
                .flat_map(|matcher| payload.evaluate(matcher))
                .collect_vec();
            let source = Source::from(&payload.prompt);
            (matches, source)
        }
        Hook::Other => (Vec::new(), Source::from("")),
    }
}

/// Build a Hook from the CLI arguments.
fn build_hook(config: &Config) -> Result<Hook> {
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

    serde_json::from_value(payload).context("failed to build UserPromptSubmit hook")
}

fn build_tool_use_hook(config: &Config) -> Result<Hook> {
    let tool = config.tool.as_deref().unwrap_or("Write");
    let file_path = config
        .file
        .as_ref()
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
        "WebFetch" => {
            let url = config
                .url
                .as_ref()
                .map(|u| u.clone())
                .unwrap_or_else(|| "https://example.com".to_string());
            json!({
                "url": url,
                "prompt": content
            })
        }
        other => bail!(
            "Unknown tool '{}'. Supported tools: Write, Edit, WebFetch",
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

    serde_json::from_value(payload).context("failed to build PreToolUse hook")
}
