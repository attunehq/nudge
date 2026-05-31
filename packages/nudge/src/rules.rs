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

/// Fully parsed Nudge configuration from one or more files.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct LoadedConfig {
    /// Rule hooks.
    pub rules: Vec<Rule>,

    /// Workflow completion gates.
    pub workflows: Vec<Workflow>,
}

impl LoadedConfig {
    fn merge(mut self, other: LoadedConfig) -> Self {
        self.rules.extend(other.rules);
        self.workflows.extend(other.workflows);
        self
    }
}

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
pub fn load_all() -> Result<LoadedConfig> {
    load_all_attributed()?
        .into_iter()
        .fold(LoadedConfig::default(), |merged, (_, config)| {
            merged.merge(config)
        })
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
pub fn load_all_attributed() -> Result<Vec<(PathBuf, LoadedConfig)>> {
    let mut configs = vec![];

    if let Some(dirs) = project_dirs() {
        let user_config = dirs.config_dir().join("rules.yaml");
        let user_config_data = load_from(&user_config)
            .with_context(|| format!("load rules from user config: {user_config:?}"))?;
        configs.push((user_config, user_config_data));
    }

    let project_root_config = PathBuf::from(".nudge.yaml");
    let project_root_config_data = load_from(&project_root_config)
        .with_context(|| format!("load rules from project root: {project_root_config:?}"))?;
    configs.push((project_root_config, project_root_config_data));

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
                let config_data = load_from(&config)
                    .with_context(|| format!("load rules from file: {config:?}"))?;
                configs.push((config, config_data));
            }
        }
    }

    Ok(configs)
}

/// Load rules from a single file.
//
// TODO: Support other file types.
#[tracing::instrument]
pub fn load_from(path: &Path) -> Result<LoadedConfig> {
    if path.extension() != Some(OsStr::new("yaml")) {
        tracing::debug!("skipping non-yaml file");
        return Ok(LoadedConfig::default());
    }

    let content = match read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(LoadedConfig::default()),
        Err(e) => return Err(e).context(format!("read config file: {path:?}")),
    };

    serde_yaml::from_str::<RuleConfig>(&content)
        .with_context(|| format!("parse config file: {path:?}"))
        .with_context(|| content.header("File content:"))
        .tap(|config| tracing::debug!(?config, "parsed config file"))
        .map(|config| LoadedConfig {
            rules: config.rules,
            workflows: config.workflows,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_file() {
        let config =
            load_from(Path::new("nonexistent.yaml")).expect("load returns empty for nonexistent");
        assert!(config.rules.is_empty());
        assert!(config.workflows.is_empty());
    }
}
