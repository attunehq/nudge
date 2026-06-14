//! Set up Nudge hooks for Codex CLI.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use clap::Args;
use color_eyre::eyre::{Context, OptionExt, Result};
use serde_json::{Value, json};
use tracing::instrument;

use crate::cmd::{command_install, json_hooks, setup_command, skill_install};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to the .codex directory.
    #[arg(long, default_value = ".codex")]
    codex_dir: PathBuf,

    /// Skip installing the bundled Nudge skill.
    #[arg(long)]
    skip_skills: bool,

    /// Skip installing bundled Nudge prompt commands.
    #[arg(long)]
    skip_commands: bool,
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    fs::create_dir_all(&config.codex_dir).context("create .codex directory")?;

    let dotcodex = config
        .codex_dir
        .canonicalize()
        .with_context(|| format!("canonicalize codex dir: {:?}", config.codex_dir))?;
    let project_root = dotcodex
        .parent()
        .ok_or_eyre("get parent directory of .codex")?;

    if !config.skip_skills {
        skill_install::install_bundled_skills(
            "Codex",
            &project_root.join(".agents").join("skills"),
        )?;
        println!();
    }

    if !config.skip_commands {
        command_install::install_codex_prompts(&dotcodex.join("prompts"))?;
        println!();
    }

    let inline_config = dotcodex.join("config.toml");
    if inline_config_has_hooks(&inline_config)? {
        eprintln!(
            "warning: {} already contains inline Codex hooks. Nudge hook setup skipped because safe TOML hook merging is out of scope. Move hooks to .codex/hooks.json or merge Nudge manually.",
            inline_config.display()
        );
        return Ok(());
    }

    let hooks_file = dotcodex.join("hooks.json");
    let nudge_command = setup_command::current_hook_command("codex")?;

    let nudge_hook = json!({
        "type": "command",
        "command": nudge_command,
        "timeout": 5,
        "statusMessage": "Checking Nudge rules"
    });
    let desired_hooks = [
        (
            "PreToolUse",
            json!({
                "matcher": "Bash|apply_patch",
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
    let mut config = if hooks_file_existed {
        let content = fs::read_to_string(&hooks_file).context("read existing hooks.json")?;
        serde_json::from_str::<Value>(&content).context("parse existing hooks.json")?
    } else {
        json!({})
    };

    let hooks = json_hooks::hooks_object(&mut config, "hooks.json")?;
    json_hooks::merge_hooks(hooks, desired_hooks)?;

    let hooks_json = serde_json::to_string_pretty(&config).context("serialize hooks.json")?;
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
    println!("1. Restart Codex sessions so hooks are loaded.");
    println!("2. Run /hooks.");
    println!("3. Review and trust the new Nudge hooks.");
    println!(
        "4. If hooks do not appear, check that the project .codex/ layer is trusted and [features].hooks has not been disabled."
    );
    println!("5. The bundled Nudge skill and prompt command will load in new Codex sessions.");

    Ok(())
}

fn inline_config_has_hooks(path: &Path) -> Result<bool> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("read .codex/config.toml"),
    };

    Ok(content.lines().any(|line| {
        let line = line.trim();
        line == "[hooks]" || line.starts_with("[hooks.")
    }))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::NamedTempFile;

    use super::inline_config_has_hooks;

    #[test]
    fn detects_inline_hooks_table() {
        let temp = NamedTempFile::new().expect("temp file");
        fs::write(temp.path(), "[hooks]\n").expect("write file");
        assert!(inline_config_has_hooks(temp.path()).expect("detect hooks"));
    }
}
