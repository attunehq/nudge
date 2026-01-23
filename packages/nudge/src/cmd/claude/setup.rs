//! Set up Nudge hooks for Claude Code.

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::Args;
use color_eyre::{
    Result,
    eyre::{Context, OptionExt, bail},
};
use nudge::claude::hook;
use serde_json::{Value, json};
use tracing::instrument;

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

This project uses [Nudge](https://github.com/attunehq/nudge), a collaborative partner that helps you remember coding conventions. Nudge watches your `Write` and `Edit` operations and reminds you about patterns and preferences that matter here—so you can focus on the actual problem instead of tracking stylistic details.

**Nudge is on your side.** When it sends you a message, it's not a reprimand—it's a colleague tapping you on the shoulder. The messages are direct (sometimes blunt) because that's what cuts through when you're focused. Trust the feedback and adjust; if a rule feels wrong, mention it so we can fix the rule.

**Writing new rules:** If the user asks you to add or modify Nudge rules, run `nudge claude docs` to see the rule format, template variables, and guidelines for writing effective messages.
"#;

#[instrument]
pub fn main(config: Config) -> Result<()> {
    fs::create_dir_all(&config.claude_dir).context("create .claude directory")?;

    let dotclaude = config
        .claude_dir
        .canonicalize()
        .with_context(|| format!("canonicalize claude dir: {:?}", config.claude_dir))?;
    let settings_file = dotclaude.join("settings.local.json");
    tracing::debug!(?dotclaude, ?settings_file, "read existing settings");

    let nudge_path = env::current_exe()
        .context("get current executable path")?
        .to_str()
        .ok_or_eyre("convert current executable path to string")?
        .to_string();

    let nudge_command = format!("{nudge_path} claude hook");
    let nudge_hook = hook::Config::builder()
        .command(nudge_command)
        .timeout(5)
        .build();
    let nudge_matcher_wildcard = hook::Matcher::builder()
        .matcher("*")
        .hooks([&nudge_hook])
        .build();
    let nudge_matcher = hook::Matcher::builder().hooks([nudge_hook]).build();
    let desired_hooks = [
        ("PreToolUse", nudge_matcher_wildcard.clone()),
        ("PostToolUse", nudge_matcher_wildcard),
        ("Stop", nudge_matcher.clone()),
        ("UserPromptSubmit", nudge_matcher),
    ];
    tracing::debug!(?desired_hooks, "generate desired hooks");

    let mut settings = if settings_file.exists() {
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

    let settings_json = serde_json::to_string_pretty(&settings).context("serialize settings")?;
    fs::write(&settings_file, settings_json).context("write settings file")?;
    tracing::debug!(?settings, ?settings_file, "wrote merged settings file");

    println!("✓ Wrote hooks configuration to {}", settings_file.display());
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
