//! User-level credentials, kept separate from repository rule configuration.

use std::{
    env, fs, io,
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::rules::project_dirs;

#[derive(Deserialize, Serialize)]
struct Credentials {
    jev: JevCredential,
}

#[derive(Deserialize, Serialize)]
struct JevCredential {
    api_key: String,
}

pub fn path() -> io::Result<PathBuf> {
    project_dirs()
        .map(|dirs| dirs.config_dir().join("credentials.json"))
        .ok_or_else(|| io::Error::other("could not locate the Nudge user configuration directory"))
}

pub fn load_jev() -> io::Result<Option<String>> {
    resolve_jev(env::var("TYPESAFE_API_KEY").ok(), path())
}

fn resolve_jev(
    environment: Option<String>,
    path: io::Result<PathBuf>,
) -> io::Result<Option<String>> {
    if let Some(key) = environment.filter(|key| !key.trim().is_empty()) {
        return Ok(Some(key));
    }
    load_jev_from(&path?)
}

fn load_jev_from(path: &Path) -> io::Result<Option<String>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(io::Error::other("could not read Nudge credentials.json")),
    };
    // Parser errors may contain credential text. Never forward them to hooks.
    let credentials = serde_json::from_str::<Credentials>(&content)
        .map_err(|_| io::Error::other("invalid Nudge credentials.json"))?;
    validate_key(&credentials.jev.api_key)?;
    Ok(Some(credentials.jev.api_key))
}

pub fn validate_key(key: &str) -> io::Result<()> {
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(io::Error::other(
            "the Jev API key must be non-empty ASCII without whitespace",
        ));
    }
    Ok(())
}

pub fn save_jev(key: &str) -> io::Result<PathBuf> {
    let path = path()?;
    save_jev_to(&path, key)?;
    Ok(path)
}

fn save_jev_to(path: &Path, key: &str) -> io::Result<()> {
    validate_key(key)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing credentials directory"))?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(parent)?;
    let credentials = Credentials {
        jev: JevCredential {
            api_key: key.to_owned(),
        },
    };
    // NamedTempFile starts owner-only on Unix; replacing the file also avoids
    // following a stale credentials symlink or retaining permissive modes.
    let mut file = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, &credentials)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq as pretty_assert_eq;
    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn environment_overrides_saved_credentials_even_when_config_is_unavailable() {
        pretty_assert_eq!(
            resolve_jev(
                Some(String::from("test-key-env")),
                Err(io::Error::other("no home"))
            )
            .expect("environment key"),
            Some(String::from("test-key-env"))
        );
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("credentials.json");
        save_jev_to(&path, "test-key-saved").expect("save");
        for environment in [None, Some(String::new()), Some(String::from("  "))] {
            pretty_assert_eq!(
                resolve_jev(environment, Ok(path.clone())).expect("saved key"),
                Some(String::from("test-key-saved"))
            );
        }
        fs::write(&path, "invalid").expect("corrupt config");
        pretty_assert_eq!(
            resolve_jev(Some(String::from("test-key-env")), Ok(path))
                .expect("override invalid config"),
            Some(String::from("test-key-env"))
        );
    }

    #[test]
    fn credentials_round_trip_replace_and_reject_invalid_input() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("nudge/credentials.json");
        pretty_assert_eq!(load_jev_from(&path).expect("missing"), None);
        save_jev_to(&path, "test-key-ada").expect("save");
        pretty_assert_eq!(
            load_jev_from(&path).expect("load").as_deref(),
            Some("test-key-ada")
        );
        for invalid in ["", "with space", "with\nnewline", "café"] {
            assert!(save_jev_to(&path, invalid).is_err());
        }
        pretty_assert_eq!(
            load_jev_from(&path).expect("preserved").as_deref(),
            Some("test-key-ada")
        );
        save_jev_to(&path, "test-key-grace").expect("replace");
        pretty_assert_eq!(
            load_jev_from(&path).expect("load").as_deref(),
            Some("test-key-grace")
        );
        fs::write(&path, r#"{"jev":{"api_key":secret-value}}"#).expect("invalid json");
        pretty_assert_eq!(
            load_jev_from(&path).expect_err("invalid").to_string(),
            "invalid Nudge credentials.json"
        );
    }

    #[test]
    #[cfg(unix)]
    fn credentials_are_private_and_replace_symlinks_without_touching_the_target() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("nudge/credentials.json");
        save_jev_to(&path, "test-key-ada").expect("save");
        pretty_assert_eq!(
            fs::metadata(&path).expect("file").permissions().mode() & 0o777,
            0o600
        );
        pretty_assert_eq!(
            fs::metadata(path.parent().expect("parent"))
                .expect("directory")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        fs::remove_file(&path).expect("remove fixture");
        let target = dir.path().join("untouched");
        fs::write(&target, "original").expect("target");
        symlink(&target, &path).expect("link");
        save_jev_to(&path, "test-key-grace").expect("replace link");
        pretty_assert_eq!(fs::read_to_string(target).expect("target"), "original");
        assert!(
            !fs::symlink_metadata(path)
                .expect("metadata")
                .file_type()
                .is_symlink()
        );
    }
}
