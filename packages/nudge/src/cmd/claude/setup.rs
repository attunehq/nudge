//! Set up Nudge hooks for Claude Code.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use bon::Builder;
use clap::Args;
use color_eyre::{
    Result,
    eyre::{Context, OptionExt},
};
use serde::Serialize;
use serde_json::{Value, json};
use tracing::instrument;

use crate::cmd::{json_hooks, setup_command};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to the .claude directory.
    #[arg(long, default_value = ".claude")]
    claude_dir: PathBuf,

    /// Skip the CLAUDE.md prompt (don't add Nudge context).
    #[arg(long)]
    skip_claude_md: bool,
}

/// The section we add to CLAUDE.md to help Claude understand Nudge's role.
const CLAUDE_MD_SECTION: &str = r#"
## Nudge

This project uses [Nudge](https://github.com/attunehq/nudge), a collaborative partner that helps you remember coding conventions and learned debugging incidents. Nudge watches supported hook surfaces, reminds you about patterns and preferences that matter here, and surfaces relevant notes from `.nudge/learned`, so you can focus on the actual problem instead of rediscovering old fixes.

**Nudge is on your side.** When it sends you a message, it's not a reprimand. It's a colleague tapping you on the shoulder. The messages are direct (sometimes blunt) because that's what cuts through when you're focused. Trust the feedback and adjust; if a rule feels wrong, mention it so we can fix the rule.

**Writing new rules or notes:** If the user asks you to add or modify Nudge rules, run `nudge claude docs` to see the rule format, template variables, and guidelines for writing effective messages. If you resolve a repo-specific bug that future agents may hit again, record it with `nudge learn add`.
"#;

/// Configures a hook in Claude Code's settings.
#[derive(Debug, Serialize, Clone, Builder)]
#[non_exhaustive]
struct HookConfig {
    /// The type of hook to run.
    ///
    /// Valid options are `command` or `prompt`, but Nudge always wants
    /// `command`: `prompt` hooks run inside Claude Code itself and cannot be
    /// intercepted by Nudge.
    #[builder(skip = String::from("command"))]
    r#type: String,

    /// The command to run.
    #[builder(into)]
    command: String,

    /// Terminate the command after this many seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u32>,
}

impl From<&HookConfig> for HookConfig {
    fn from(value: &HookConfig) -> Self {
        value.clone()
    }
}

/// Configures hook matching strategy in Claude Code's settings.local.json.
#[derive(Debug, Serialize, Clone, Builder)]
#[non_exhaustive]
struct HookMatcher {
    /// The matcher to use for tool hooks.
    #[builder(default = "", into)]
    #[serde(skip_serializing_if = "String::is_empty")]
    matcher: String,

    /// The hooks to run when the matcher matches.
    #[builder(with = |i: impl IntoIterator<Item = impl Into<HookConfig>>| i.into_iter().map(Into::into).collect())]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hooks: Vec<HookConfig>,
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    fs::create_dir_all(&config.claude_dir).context("create .claude directory")?;

    let dotclaude = config
        .claude_dir
        .canonicalize()
        .with_context(|| format!("canonicalize claude dir: {:?}", config.claude_dir))?;
    let settings_file = dotclaude.join("settings.local.json");
    tracing::debug!(?dotclaude, ?settings_file, "read existing settings");

    let nudge_command = setup_command::current_hook_command("claude")?;
    let nudge_hook = HookConfig::builder()
        .command(&nudge_command)
        .timeout(5)
        .build();
    let nudge_pretooluse_matcher = HookMatcher::builder()
        .matcher("Write|Edit|WebFetch|Bash")
        .hooks([&nudge_hook])
        .build();
    let nudge_matcher = HookMatcher::builder().hooks([nudge_hook]).build();
    let desired_hooks = [
        ("PreToolUse", nudge_pretooluse_matcher),
        ("UserPromptSubmit", nudge_matcher),
    ];
    tracing::debug!(?desired_hooks, "generate desired hooks");

    let settings_existed = settings_file.exists();
    let mut settings = if settings_existed {
        let content =
            fs::read_to_string(&settings_file).context("read existing settings.local.json")?;
        serde_json::from_str::<Value>(&content).context("parse existing settings.local.json")?
    } else {
        serde_json::json!({})
    };
    tracing::debug!(?settings, "read existing settings");

    // Merge hooks into settings, avoiding duplicates.
    //
    // We work with the settings as a `Value` so that we avoid clobbering
    // existing settings that we may not know about. We've enabled the
    // `preserve_order` feature of serde_json; this should reduce the impact of
    // our changes on the user's settings file.
    {
        let hooks = json_hooks::hooks_object(&mut settings, "settings")?;
        let desired_hooks = desired_hooks
            .into_iter()
            .map(|(event, matcher)| (event, json!(matcher)));
        json_hooks::merge_hooks(hooks, desired_hooks)?;
        remove_managed_event_hook(hooks, "PostToolUse", &nudge_command);
        remove_managed_event_hook(hooks, "Stop", &nudge_command);
    }

    let settings_json = serde_json::to_string_pretty(&settings).context("serialize settings")?;
    let backup_path = if settings_existed {
        setup_command::backup_existing_file(&settings_file)?
    } else {
        None
    };
    fs::write(&settings_file, settings_json).context("write settings file")?;
    tracing::debug!(?settings, ?settings_file, "wrote merged settings file");

    println!("✓ Wrote hooks configuration to {}", settings_file.display());
    if let Some(backup_path) = backup_path {
        println!(
            "  Backed up previous configuration to {}",
            backup_path.display()
        );
    }
    println!();

    if !config.skip_claude_md {
        let project_root = dotclaude
            .parent()
            .ok_or_eyre("get parent directory of .claude")?;
        offer_claude_md_section(project_root)?;
    }

    println!("Next steps:");
    println!("1. Run /hooks in Claude Code to verify hooks are registered");
    println!("2. Use claude --debug to see hook execution logs");

    Ok(())
}

fn remove_managed_event_hook(
    hooks: &mut serde_json::Map<String, Value>,
    event: &str,
    command: &str,
) {
    let Some(entry) = hooks.get_mut(event) else {
        return;
    };
    let Value::Array(matchers) = entry else {
        return;
    };

    matchers.retain(|matcher| !matcher_uses_command(matcher, command));
    if matchers.is_empty() {
        hooks.remove(event);
    }
}

fn matcher_uses_command(matcher: &Value, command: &str) -> bool {
    matcher
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("type").and_then(Value::as_str) == Some("command")
                    && hook.get("command").and_then(Value::as_str) == Some(command)
            })
        })
}

/// Offer to add a Nudge section to CLAUDE.md in the project root.
fn offer_claude_md_section(project_root: &std::path::Path) -> Result<()> {
    let claude_md_path = project_root.join("CLAUDE.md");
    if claude_md_path.exists() {
        let content = fs::read_to_string(&claude_md_path).context("read existing CLAUDE.md")?;
        if content.contains("## Nudge") {
            println!("CLAUDE.md already has a Nudge section, skipping.");
            println!();
            return Ok(());
        }
    }

    // Show the user what we'll add and why
    println!("─────────────────────────────────────────────────────────────────────");
    println!();
    println!("Nudge works best when Claude understands it's a collaborative partner,");
    println!("not a rule enforcer. Adding context to CLAUDE.md helps set the right tone.");
    println!();
    println!("This will be added to {}:", claude_md_path.display());
    println!();
    for line in CLAUDE_MD_SECTION.lines() {
        println!("  {line}");
    }
    println!();
    println!("─────────────────────────────────────────────────────────────────────");
    println!();

    // Prompt for confirmation
    print!("Add this section to CLAUDE.md? [Y/n] ");
    io::stdout().flush().context("flush stdout")?;

    let stdin = io::stdin();
    let mut line = String::new();
    stdin
        .lock()
        .read_line(&mut line)
        .context("read user input")?;
    let response = line.trim().to_lowercase();

    if response.is_empty() || response == "y" || response == "yes" {
        let mut content = if claude_md_path.exists() {
            let existing =
                fs::read_to_string(&claude_md_path).context("read existing CLAUDE.md")?;

            if existing.ends_with('\n') {
                existing
            } else {
                format!("{existing}\n")
            }
        } else {
            String::from(
                "# CLAUDE.md\n\nThis file provides guidance to Claude Code when working with code in this repository.\n",
            )
        };

        content.push_str(CLAUDE_MD_SECTION);

        fs::write(&claude_md_path, content).context("write CLAUDE.md")?;
        println!("✓ Added Nudge section to {}", claude_md_path.display());
        println!();
    } else {
        println!("Skipped CLAUDE.md update.");
        println!();
    }

    Ok(())
}
