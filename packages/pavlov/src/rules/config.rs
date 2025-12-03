//! Config file discovery and YAML parsing.

use std::ffi::OsStr;
use std::path::Path;

use color_eyre::eyre::{Context, Result};
use walkdir::WalkDir;

use super::schema::{Rule, RuleConfig};

/// Load all rules from all sources (purely additive).
///
/// Loading order:
/// 1. `~/.config/pavlov/rules.yaml` if it exists
/// 2. `.pavlov.yaml` if it exists
/// 3. `.pavlov/` directory walked recursively (sorted), loading all `*.yaml` files
pub fn load_all_rules() -> Result<Vec<Rule>> {
    let mut rules = vec![];

    // 1. User-level rules
    if let Some(config_dir) = dirs::config_dir() {
        let path = config_dir.join("pavlov/rules.yaml");
        rules.extend(load_rules_from_file(&path)?);
    }

    // 2. Project-level single file
    rules.extend(load_rules_from_file(Path::new(".pavlov.yaml"))?);

    // 3. Project-level directory (walk all .yaml files in sorted order)
    let pavlov_dir = Path::new(".pavlov");
    if pavlov_dir.is_dir() {
        for entry in WalkDir::new(pavlov_dir)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(OsStr::new("yaml")))
            .filter(|e| e.file_type().is_file())
        {
            rules.extend(load_rules_from_file(entry.path())?);
        }
    }

    Ok(rules)
}

/// Load rules from a single YAML file.
pub fn load_rules_from_file(path: &Path) -> Result<Vec<Rule>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let config: RuleConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    Ok(config.rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_file() {
        let rules = load_rules_from_file(Path::new("nonexistent.yaml")).unwrap();
        assert!(rules.is_empty());
    }
}
