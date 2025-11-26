//! Set up Pavlov hooks for Claude Code.

use std::fs;
use std::path::PathBuf;

use clap::Args;
use color_eyre::{
    Result,
    eyre::{Context, OptionExt, bail},
};
use pavlov::claude::hook;
use serde_json::{Value, json};
use tracing::instrument;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to the .claude directory.
    #[arg(long, default_value = ".claude")]
    claude_dir: PathBuf,
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    // Create .claude directory if it doesn't exist (must happen before canonicalize)
    fs::create_dir_all(&config.claude_dir).context("create .claude directory")?;

    let dotclaude = config
        .claude_dir
        .canonicalize()
        .context("canonicalize claude dir")?;
    let settings_file = dotclaude.join("settings.json");
    tracing::debug!(?dotclaude, ?settings_file, "read existing settings");

    // Get the path to the pavlov binary
    let pavlov_path = std::env::current_exe()
        .context("get current executable path")?
        .to_str()
        .ok_or_eyre("convert current executable path to string")?
        .to_string();

    // Set up the desired hooks for the settings file.
    let pavlov_command = format!("{pavlov_path} claude hook");
    let pavlov_hook = hook::Config::builder()
        .command(pavlov_command)
        .timeout(5)
        .build();
    let pavlov_matcher_wildcard = hook::Matcher::builder()
        .matcher("*")
        .hooks([&pavlov_hook])
        .build();
    let pavlov_matcher = hook::Matcher::builder().hooks([pavlov_hook]).build();
    let desired_hooks = [
        ("PreToolUse", pavlov_matcher_wildcard.clone()),
        ("PostToolUse", pavlov_matcher_wildcard),
        ("Stop", pavlov_matcher.clone()),
        ("UserPromptSubmit", pavlov_matcher),
    ];
    tracing::debug!(?desired_hooks, "generate desired hooks");

    // Read existing settings if they exist
    let mut settings = if settings_file.exists() {
        let content = fs::read_to_string(&settings_file).context("read existing settings.json")?;
        serde_json::from_str::<Value>(&content).context("parse existing settings.json")?
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
    //
    // TODO: we might want to warn the user so that we don't clobber their
    // comments or whatever, or at least back up their existing settings file
    // and leave it behind for them to merge with our changes if desired.
    let Value::Object(settings) = &mut settings else {
        bail!("expected settings to be an object, got: {settings:?}");
    };
    let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
    let Value::Object(hooks) = hooks else {
        bail!("expected hooks to be an object, got: {hooks:?}");
    };
    for (event, matcher) in desired_hooks {
        let entry = hooks.entry(event).or_insert_with(|| json!([]));
        let Value::Array(matchers) = entry else {
            bail!("expected matchers to be an array, got: {entry:?}");
        };
        let matcher = json!(matcher);
        tracing::debug!(?event, ?matcher, ?matchers, "merge hooks");
        if !matchers.contains(&matcher) {
            matchers.push(matcher);
        }
    }

    // We're done, now we write the settings and tell the user.
    let settings_json = serde_json::to_string_pretty(&settings).context("serialize settings")?;
    fs::write(&settings_file, settings_json).context("write settings file")?;
    tracing::debug!(?settings, ?settings_file, "wrote merged settings file");

    println!("✓ Wrote hooks configuration to {}", settings_file.display());
    println!();
    println!("Next steps:");
    println!("1. Run /hooks in Claude Code to verify hooks are registered");
    println!("2. Use claude --debug to see hook execution logs");

    Ok(())
}
