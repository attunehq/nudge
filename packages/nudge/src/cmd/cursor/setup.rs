//! Set up Nudge hooks for Cursor / cursor-agent.

use std::fs;
use std::path::PathBuf;

use clap::Args;
use color_eyre::{Result, eyre::Context};
use serde_json::{Value, json};
use tracing::instrument;

use crate::cmd::{json_hooks, setup_command, skill_install};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to the .cursor directory.
    #[arg(long, default_value = ".cursor")]
    cursor_dir: PathBuf,

    /// Skip installing the bundled Nudge skills.
    #[arg(long)]
    skip_skills: bool,
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    fs::create_dir_all(&config.cursor_dir).context("create .cursor directory")?;

    let dotcursor = config
        .cursor_dir
        .canonicalize()
        .with_context(|| format!("canonicalize cursor dir: {:?}", config.cursor_dir))?;

    if !config.skip_skills {
        skill_install::install_bundled_skills("Cursor", &dotcursor.join("skills"))?;
        println!();
    }

    let hooks_file = dotcursor.join("hooks.json");
    let nudge_command = setup_command::current_hook_command("cursor")?;

    let desired_hooks = [
        (
            "preToolUse",
            json!({
                "command": nudge_command,
                "timeout": 5,
                "matcher": "Shell|Write|Delete|WebFetch"
            }),
        ),
        (
            "beforeShellExecution",
            json!({
                "command": nudge_command,
                "timeout": 5
            }),
        ),
        (
            "beforeSubmitPrompt",
            json!({
                "command": nudge_command,
                "timeout": 5
            }),
        ),
    ];

    let hooks_file_existed = hooks_file.exists();
    let mut hooks_config = if hooks_file_existed {
        let content = fs::read_to_string(&hooks_file).context("read existing hooks.json")?;
        serde_json::from_str::<Value>(&content).context("parse existing hooks.json")?
    } else {
        json!({ "version": 1 })
    };

    if hooks_config.get("version").is_none() {
        hooks_config["version"] = json!(1);
    }

    let hooks = json_hooks::hooks_object(&mut hooks_config, "hooks.json")?;
    json_hooks::merge_hooks(hooks, desired_hooks)?;

    let hooks_json = serde_json::to_string_pretty(&hooks_config).context("serialize hooks.json")?;
    let backup_path = if hooks_file_existed {
        setup_command::backup_existing_file(&hooks_file)?
    } else {
        None
    };
    fs::write(&hooks_file, hooks_json).context("write hooks.json")?;

    println!("✓ Wrote hooks configuration to {}", hooks_file.display());
    if let Some(backup_path) = backup_path {
        println!(
            "  Backed up previous configuration to {}",
            backup_path.display()
        );
    }
    println!();
    println!("Next steps:");
    println!("1. Restart Cursor / cursor-agent sessions so hooks are loaded.");
    println!("2. Trust the workspace if Cursor prompts before project hooks can run.");
    println!("3. Open the Hooks output channel to confirm Nudge is loaded.");
    println!("4. The bundled Nudge skills will load from .cursor/skills in new Cursor sessions.");

    Ok(())
}
