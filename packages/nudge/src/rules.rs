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
/// 3. `.nudge.yml` if it exists
/// 4. `.nudge/` directory walked recursively, loading all `*.yaml` and `*.yml` files
#[tracing::instrument]
pub fn load_all() -> Result<Vec<Rule>> {
    load_all_attributed()?
        .into_iter()
        .flat_map(|(_, rules)| rules)
        .collect::<Vec<_>>()
        .pipe(Ok)
}

/// Load all rules from all sources, returning each set of rules with the path
/// to the config file that contained them.
///
/// Loading order (all additive):
/// 1. User-level rules from `ProjectDirs::config_dir()/rules.yaml` if it exists
/// 2. `.nudge.yaml` if it exists
/// 3. `.nudge.yml` if it exists
/// 4. `.nudge/` directory walked recursively, loading all `*.yaml` and `*.yml` files
#[tracing::instrument]
pub fn load_all_attributed() -> Result<Vec<(PathBuf, Vec<Rule>)>> {
    load_all_configs_attributed().map(|configs| {
        configs
            .into_iter()
            .map(|(path, config)| (path, config.rules))
            .collect()
    })
}

/// Load all Nudge configuration files from all sources.
#[tracing::instrument]
pub fn load_all_configs_attributed() -> Result<Vec<(PathBuf, RuleConfig)>> {
    let mut configs = vec![];

    if let Some(dirs) = project_dirs() {
        let user_config = dirs.config_dir().join("rules.yaml");
        if let Some(config) = load_config_from(&user_config)
            .with_context(|| format!("load config from user config: {user_config:?}"))?
        {
            configs.push((user_config, config));
        }
    }

    for project_root_config in [PathBuf::from(".nudge.yaml"), PathBuf::from(".nudge.yml")] {
        if let Some(config) = load_config_from(&project_root_config)
            .with_context(|| format!("load config from project root: {project_root_config:?}"))?
        {
            configs.push((project_root_config, config));
        }
    }

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
                if let Some(config_content) = load_config_from(&config)
                    .with_context(|| format!("load config from file: {config:?}"))?
                {
                    configs.push((config, config_content));
                }
            }
        }
    }

    Ok(configs)
}

/// Load rules from a single file.
//
// TODO: Support other file types.
#[tracing::instrument]
pub fn load_from(path: &Path) -> Result<Vec<Rule>> {
    Ok(load_config_from(path)?
        .map(|config| config.rules)
        .unwrap_or_default())
}

/// Load a single Nudge config file.
#[tracing::instrument]
pub fn load_config_from(path: &Path) -> Result<Option<RuleConfig>> {
    if !is_config_extension(path.extension()) {
        tracing::debug!("skipping non-yaml file");
        return Ok(None);
    }

    let content = match read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context(format!("read config file: {path:?}")),
    };

    serde_yaml::from_str::<RuleConfig>(&content)
        .with_context(|| format!("parse config file: {path:?}"))
        .with_context(|| content.header("File content:"))
        .tap(|config| tracing::debug!(?config, "parsed config file"))
        .map(Some)
}

fn is_config_extension(extension: Option<&OsStr>) -> bool {
    extension == Some(OsStr::new("yaml")) || extension == Some(OsStr::new("yml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_file() {
        let rules =
            load_from(Path::new("nonexistent.yaml")).expect("load returns empty for nonexistent");
        assert!(rules.is_empty());
    }
}
