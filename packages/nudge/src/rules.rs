//! Rule data types and loading operations.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::{ffi::OsStr, fs::read_to_string};

use color_eyre::{
    SectionExt,
    eyre::{Context, Result},
};
use directories::ProjectDirs;
use tap::{Pipe, Tap};
use walkdir::WalkDir;

pub use schema::*;

mod schema;

/// Get the project directories for the application.
#[tracing::instrument]
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "attunehq", "nudge")
}

/// Load all rules from all sources.
///
/// Loading order (all additive):
/// 1. User-level rules from `ProjectDirs::config_dir()/rules.yaml` if it exists
/// 2. `.nudge.yaml` if it exists
/// 3. `.nudge/` directory walked recursively, loading all `*.yaml` files
#[tracing::instrument]
pub fn load_all() -> Result<Vec<Rule>> {
    load_all_attributed()?
        .into_iter()
        .map(|(_, rules)| rules)
        .flatten()
        .collect::<Vec<_>>()
        .pipe(Ok)
}

/// Load all rules from all sources, returning each set of rules with the path
/// to the config file that contained them.
///
/// Loading order (all additive):
/// 1. User-level rules from `ProjectDirs::config_dir()/rules.yaml` if it exists
/// 2. `.nudge.yaml` if it exists
/// 3. `.nudge/` directory walked recursively, loading all `*.yaml` files
#[tracing::instrument]
pub fn load_all_attributed() -> Result<Vec<(PathBuf, Vec<Rule>)>> {
    let mut rules = vec![];

    if let Some(dirs) = project_dirs() {
        let user_config = dirs.config_dir().join("rules.yaml");
        let user_rules = load_from(&user_config)
            .with_context(|| format!("load rules from user config: {user_config:?}"))?;
        rules.push((user_config, user_rules));
    }

    let project_root_config = PathBuf::from(".nudge.yaml");
    let project_root_rules = load_from(&project_root_config)
        .with_context(|| format!("load rules from project root: {project_root_config:?}"))?;
    rules.push((project_root_config, project_root_rules));

    let root = Path::new(".nudge");
    if root.is_dir() {
        for entry in WalkDir::new(root).sort_by_file_name().into_iter() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(?error, ?root, "walking directory");
                    continue;
                }
            };

            if entry.file_type().is_file() {
                let config = entry.path().to_path_buf();
                let config_rules = load_from(&config)
                    .with_context(|| format!("load rules from file: {config:?}"))?;
                rules.push((config, config_rules));
            }
        }
    }

    Ok(rules)
}

/// Load rules from a single file.
//
// TODO: Support other file types.
#[tracing::instrument]
pub fn load_from(path: &Path) -> Result<Vec<Rule>> {
    if path.extension() != Some(OsStr::new("yaml")) {
        tracing::debug!("skipping non-yaml file");
        return Ok(vec![]);
    }

    let content = match read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e).context(format!("read config file: {path:?}")),
    };

    serde_yaml::from_str::<RuleConfig>(&content)
        .with_context(|| format!("parse config file: {path:?}"))
        .with_context(|| content.header("File content:"))
        .tap(|config| tracing::debug!(?config, "parsed config file"))
        .map(|config| config.rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_file() {
        let rules = load_from(Path::new("nonexistent.yaml")).unwrap();
        assert!(rules.is_empty());
    }
}
