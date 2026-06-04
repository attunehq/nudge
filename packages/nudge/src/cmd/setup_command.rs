//! Shared setup helpers for provider hook commands.

use std::{
    env,
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Context, OptionExt, Result};

pub(crate) fn current_hook_command(provider: &str) -> Result<String> {
    let nudge_path = env::current_exe().context("get current executable path")?;
    hook_command(&nudge_path, provider)
}

pub(crate) fn backup_existing_file(path: &Path) -> Result<Option<PathBuf>> {
    if !path
        .try_exists()
        .with_context(|| format!("check whether {} exists", path.display()))?
    {
        return Ok(None);
    }

    let mut source = File::open(path).with_context(|| format!("open {}", path.display()))?;
    for suffix in 0.. {
        let backup_path = backup_path(path, suffix);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup_path)
        {
            Ok(mut backup) => {
                io::copy(&mut source, &mut backup).with_context(|| {
                    format!(
                        "copy {} to backup {}",
                        path.display(),
                        backup_path.display()
                    )
                })?;
                return Ok(Some(backup_path));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("create backup {}", backup_path.display()));
            }
        }
    }

    unreachable!("unbounded backup suffix search should always return or error")
}

fn backup_path(path: &Path, suffix: u32) -> PathBuf {
    let mut backup = path.as_os_str().to_os_string();
    backup.push(".bak");
    if suffix > 0 {
        backup.push(format!(".{suffix}"));
    }
    PathBuf::from(backup)
}

fn hook_command(nudge_path: &Path, provider: &str) -> Result<String> {
    let nudge_path = nudge_path
        .to_str()
        .ok_or_eyre("convert current executable path to string")?;
    Ok(shell_words::join([nudge_path, provider, "hook"]))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use pretty_assertions::assert_eq as pretty_assert_eq;
    use tempfile::TempDir;

    use super::{backup_existing_file, hook_command};

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

    #[test]
    fn backup_existing_file_uses_next_available_suffix() {
        let temp = TempDir::new().expect("temp dir");
        let target = temp.path().join("hooks.json");
        let first_backup = temp.path().join("hooks.json.bak");
        fs::write(&target, "original").expect("write target");
        fs::write(&first_backup, "first backup").expect("write first backup");

        let backup = backup_existing_file(&target)
            .expect("backup")
            .expect("existing file should get backup");

        pretty_assert_eq!(backup, temp.path().join("hooks.json.bak.1"));
        pretty_assert_eq!(
            fs::read_to_string(&first_backup).expect("read first backup"),
            "first backup"
        );
        pretty_assert_eq!(fs::read_to_string(backup).expect("read backup"), "original");
    }
}
