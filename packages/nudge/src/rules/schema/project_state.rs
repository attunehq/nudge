use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

use super::ContentMatcher;

/// Matcher for project state conditions.
///
/// Project state matchers evaluate conditions about the project environment
/// rather than the content of the tool input. All project state matchers in a
/// rule must match for the rule to proceed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum ProjectStateMatcher {
    /// Match against git repository state.
    Git {
        /// Match against the current branch name.
        #[serde(default)]
        branch: Vec<ContentMatcher>,
    },
}

impl<'de> Deserialize<'de> for ProjectStateMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            kind: String,
            #[serde(default)]
            branch: Vec<ContentMatcher>,
        }

        let raw = Raw::deserialize(deserializer)?;

        match raw.kind.as_str() {
            "Git" => Ok(ProjectStateMatcher::Git { branch: raw.branch }),
            other => Err(serde::de::Error::unknown_variant(other, &["Git"])),
        }
    }
}

impl ProjectStateMatcher {
    /// Evaluate this matcher against the project state at the given path.
    pub fn is_match(&self, cwd: &Path) -> bool {
        match self {
            ProjectStateMatcher::Git { branch } => {
                let Some(current_branch) = crate::git::current_branch(cwd) else {
                    tracing::warn!(?cwd, "project_state.Git matcher: not in a git repository");
                    return false;
                };

                branch.is_empty() || branch.iter().all(|m| m.is_match(&current_branch))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_state_git_deserialize() {
        let yaml = r#"
            kind: Git
            branch:
              - kind: Regex
                pattern: "^main$"
        "#;
        let matcher = serde_yaml::from_str::<ProjectStateMatcher>(yaml)
            .expect("valid project state matcher yaml");
        assert!(matches!(matcher, ProjectStateMatcher::Git { .. }));
    }

    #[test]
    fn test_project_state_git_empty_branch() {
        let yaml = r#"
            kind: Git
            branch: []
        "#;
        let matcher = serde_yaml::from_str::<ProjectStateMatcher>(yaml)
            .expect("valid project state matcher yaml");
        let ProjectStateMatcher::Git { branch } = matcher;
        assert!(branch.is_empty());
    }

    #[test]
    fn test_project_state_invalid_kind() {
        let yaml = r#"
            kind: InvalidKind
        "#;
        let result = serde_yaml::from_str::<ProjectStateMatcher>(yaml);
        assert!(result.is_err());
    }
}
