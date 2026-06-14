//! Set up Nudge hooks for Claude Code.

use std::fs;
use std::path::PathBuf;

use bon::Builder;
use clap::Args;
use color_eyre::{Result, eyre::Context};
use serde::Serialize;
use serde_json::{Value, json};
use tracing::instrument;

use crate::cmd::{command_install, json_hooks, setup_command, skill_install};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to the .claude directory.
    #[arg(long, default_value = ".claude")]
    claude_dir: PathBuf,

    /// Skip installing the bundled Nudge skill.
    #[arg(long)]
    skip_skills: bool,

    /// Skip installing bundled Nudge slash commands.
    #[arg(long)]
    skip_commands: bool,
}

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

    if !config.skip_skills {
        skill_install::install_bundled_skills("Claude", &dotclaude.join("skills"))?;
        println!();
    }

    if !config.skip_commands {
        command_install::install_claude_commands(&dotclaude.join("commands"))?;
        println!();
    }

    println!("Next steps:");
    println!("1. Run /hooks in Claude Code to verify hooks are registered");
    println!("2. Use claude --debug to see hook execution logs");
    println!("3. Restart Claude Code so hooks, skills, and slash commands are loaded");

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
