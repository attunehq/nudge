//! Config file discovery and YAML parsing.

use std::io::ErrorKind;
use std::path::Path;
use std::{ffi::OsStr, fs::read_to_string};

use color_eyre::{
    SectionExt,
    eyre::{Context, Result},
};
use directories::ProjectDirs;
use tap::Tap;
use walkdir::WalkDir;

use super::schema::{Rule, RuleConfig};

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
pub fn load_rules() -> Result<Vec<Rule>> {
    let mut rules = vec![];

    if let Some(dirs) = project_dirs() {
        let user_config = dirs.config_dir().join("rules.yaml");
        let user_rules = load_rules_from(&user_config)
            .with_context(|| format!("load rules from user config: {user_config:?}"))?;
        rules.extend(user_rules);
    }

    let project_root_config = Path::new(".nudge.yaml");
    let project_root_rules = load_rules_from(project_root_config)
        .with_context(|| format!("load rules from project root: {project_root_config:?}"))?;
    rules.extend(project_root_rules);

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
                let config = entry.path();
                let config_rules = load_rules_from(config)
                    .with_context(|| format!("load rules from file: {config:?}"))?;
                rules.extend(config_rules);
            }
        }
    }

    Ok(rules)
}

/// Load rules from a single file.
//
// TODO: Support other file types.
#[tracing::instrument]
pub fn load_rules_from(path: &Path) -> Result<Vec<Rule>> {
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
        let rules = load_rules_from(Path::new("nonexistent.yaml")).unwrap();
        assert!(rules.is_empty());
    }
}
