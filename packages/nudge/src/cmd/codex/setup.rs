//! Set up Nudge hooks for Codex CLI.

use std::{env, fs, path::PathBuf};

use clap::Args;
use color_eyre::eyre::{Context, OptionExt, Result, bail};
use serde_json::{Value, json};
use tracing::instrument;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to the .codex directory.
    #[arg(long, default_value = ".codex")]
    codex_dir: PathBuf,
}

#[instrument]
pub fn main(config: Config) -> Result<()> {
    fs::create_dir_all(&config.codex_dir).context("create .codex directory")?;

    let dotcodex = config
        .codex_dir
        .canonicalize()
        .with_context(|| format!("canonicalize codex dir: {:?}", config.codex_dir))?;

    let inline_config = dotcodex.join("config.toml");
    if inline_config_has_hooks(&inline_config)? {
        eprintln!(
            "warning: {} already contains inline Codex hooks. Nudge setup skipped because safe TOML hook merging is out of scope. Move hooks to .codex/hooks.json or merge Nudge manually.",
            inline_config.display()
        );
        return Ok(());
    }

    let hooks_file = dotcodex.join("hooks.json");
    let nudge_path = env::current_exe()
        .context("get current executable path")?
        .to_str()
        .ok_or_eyre("convert current executable path to string")?
        .to_string();
    let nudge_command = format!("{nudge_path} codex hook");

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

    let mut config = if hooks_file.exists() {
        let content = fs::read_to_string(&hooks_file).context("read existing hooks.json")?;
        serde_json::from_str::<Value>(&content).context("parse existing hooks.json")?
    } else {
        json!({})
    };

    let Value::Object(root) = &mut config else {
        bail!("expected hooks.json to be an object, got: {config:?}");
    };
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let Value::Object(hooks) = hooks else {
        bail!("expected hooks to be an object, got: {hooks:?}");
    };

    for (event, matcher) in desired_hooks {
        let entry = hooks.entry(event).or_insert_with(|| json!([]));
        let Value::Array(matchers) = entry else {
            bail!("expected hook matchers to be an array, got: {entry:?}");
        };

        if !matchers.contains(&matcher) {
            matchers.push(matcher);
        }
    }

    let hooks_json = serde_json::to_string_pretty(&config).context("serialize hooks.json")?;
    fs::write(&hooks_file, hooks_json).context("write hooks.json")?;

    println!("✓ Wrote hooks configuration to {}", hooks_file.display());
    println!();
    println!("Next steps:");
    println!("1. Restart Codex sessions so hooks are loaded.");
    println!("2. Run /hooks.");
    println!("3. Review and trust the new Nudge hooks.");
    println!(
        "4. If hooks do not appear, check that the project .codex/ layer is trusted and [features].hooks has not been disabled."
    );

    Ok(())
}

fn inline_config_has_hooks(path: &std::path::Path) -> Result<bool> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("read .codex/config.toml"),
    };

    Ok(content.lines().any(|line| {
        let line = line.trim();
        line == "[hooks]" || line.starts_with("[hooks.")
    }))
}

#[cfg(test)]
mod tests {
    use super::inline_config_has_hooks;

    #[test]
    fn detects_inline_hooks_table() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), "[hooks]\n").expect("write file");
        assert!(inline_config_has_hooks(temp.path()).expect("detect hooks"));
    }
}
