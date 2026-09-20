//! Set up Nudge hooks for Grok Build.

use std::fs;
use std::path::PathBuf;

use clap::Args;
use color_eyre::{Result, eyre::Context};
use serde_json::{Value, json};
use tracing::instrument;

use crate::cmd::{json_hooks, setup_command, skill_install};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to the .grok directory.
    #[arg(long, default_value = ".grok")]
    grok_dir: PathBuf,

    /// Skip installing the bundled Nudge skills.
    #[arg(long)]
    skip_skills: bool,
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    let hooks_dir = config.grok_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).context("create .grok/hooks directory")?;

    let dotgrok = config
        .grok_dir
        .canonicalize()
        .with_context(|| format!("canonicalize grok dir: {:?}", config.grok_dir))?;

    if !config.skip_skills {
        skill_install::install_bundled_skills("Grok", &dotgrok.join("skills"))?;
        println!();
    }

    let hooks_file = dotgrok.join("hooks").join("nudge.json");
    let nudge_command = setup_command::current_hook_command("grok")?;

    let nudge_hook = json!({
        "type": "command",
        "command": nudge_command,
        "timeout": 5
    });
    let desired_hooks = [
        (
            "PreToolUse",
            json!({
                "matcher": "Write|Edit|MultiEdit|WebFetch|Bash|run_terminal_command|search_replace|write|write_file|create_file|edit_file|web_fetch",
                "hooks": [nudge_hook.clone()]
            }),
        ),
        (
            "UserPromptSubmit",
            json!({
                "hooks": [nudge_hook]
            }),
        ),
    ];

    let hooks_file_existed = hooks_file.exists();
    let mut hooks_config = if hooks_file_existed {
        let content = fs::read_to_string(&hooks_file).context("read existing nudge.json")?;
        serde_json::from_str::<Value>(&content).context("parse existing nudge.json")?
    } else {
        json!({})
    };

    let hooks = json_hooks::hooks_object(&mut hooks_config, "nudge.json")?;
    json_hooks::merge_hooks(hooks, desired_hooks)?;

    let hooks_json = serde_json::to_string_pretty(&hooks_config).context("serialize nudge.json")?;
    let backup_path = if hooks_file_existed {
        setup_command::backup_existing_file(&hooks_file)?
    } else {
        None
    };
    fs::write(&hooks_file, hooks_json).context("write nudge.json")?;

    println!("✓ Wrote hooks configuration to {}", hooks_file.display());
    if let Some(backup_path) = backup_path {
        println!(
            "  Backed up previous configuration to {}",
            backup_path.display()
        );
    }
    println!();
    println!("Next steps:");
    println!("1. Restart Grok Build sessions so hooks are loaded.");
    println!("2. Run /hooks-trust or launch with --trust so project hooks can run.");
    println!("3. Run /hooks to confirm Nudge is loaded.");
    println!("4. The bundled Nudge skills will load from .grok/skills in new Grok sessions.");

    Ok(())
}
