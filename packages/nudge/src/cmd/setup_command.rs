//! Shared setup helpers for provider hook commands.

use std::{env, path::Path};

use color_eyre::eyre::{Context, OptionExt, Result};

pub(crate) fn current_hook_command(provider: &str) -> Result<String> {
    let nudge_path = env::current_exe().context("get current executable path")?;
    hook_command(&nudge_path, provider)
}

fn hook_command(nudge_path: &Path, provider: &str) -> Result<String> {
    let nudge_path = nudge_path
        .to_str()
        .ok_or_eyre("convert current executable path to string")?;
    Ok(shell_words::join([nudge_path, provider, "hook"]))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::hook_command;

    #[test]
    fn hook_command_quotes_paths_for_shell_execution() {
        let command = hook_command(
            Path::new("/tmp/Nudge Test/bin/nudge's debug build"),
            "claude",
        )
        .expect("build command");

        assert!(command.starts_with("'/tmp/Nudge Test/bin/nudge"));
        let words = shell_words::split(&command).expect("split command");
        pretty_assert_eq!(
            words,
            vec!["/tmp/Nudge Test/bin/nudge's debug build", "claude", "hook"]
        );
    }
}
