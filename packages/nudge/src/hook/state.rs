//! Local interaction state for context-aware prompt reminders.

use std::{
    collections::BTreeMap,
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use color_eyre::{
    Result,
    eyre::{Context, OptionExt},
};
use serde::{Deserialize, Serialize};

use crate::{
    hook::{NudgeHook, PreToolUse, ToolUse},
    rules::{FileChangeMatcher, Rule, project_dirs},
};

const STATE_FILE_NAME: &str = "interaction-state.json";
const STATE_VERSION: u8 = 1;
const MAX_FILE_CHANGES_PER_PROJECT: usize = 512;
const MAX_FILE_CHANGE_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Local state used by stateful `UserPromptSubmit` matchers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionState {
    version: u8,

    #[serde(default)]
    projects: BTreeMap<String, ProjectInteractionState>,
}

impl Default for InteractionState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            projects: BTreeMap::new(),
        }
    }
}

impl InteractionState {
    /// Load interaction state from the local machine.
    pub fn load() -> Result<Self> {
        let Some(path) = state_file_path()? else {
            return Ok(Self::default());
        };

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };

        serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))
    }

    /// Save interaction state to the local machine.
    pub fn save(&self) -> Result<()> {
        let Some(path) = state_file_path()? else {
            return Ok(());
        };

        let parent = path
            .parent()
            .ok_or_eyre("interaction state path has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        let content = serde_json::to_string_pretty(self).context("serialize interaction state")?;
        fs::write(&path, content).with_context(|| format!("write {}", path.display()))
    }

    /// Record allowed file changes that match at least one stateful prompt
    /// rule.
    pub fn record_file_changes(&mut self, hooks: &[NudgeHook], rules: &[Rule], now: u64) {
        let file_gates = rules
            .iter()
            .flat_map(Rule::hooks_userpromptsubmit)
            .flat_map(|matcher| matcher.after_file_change.iter())
            .collect::<Vec<_>>();
        if file_gates.is_empty() {
            return;
        }

        for hook in hooks {
            let NudgeHook::PreToolUse(payload) = hook else {
                continue;
            };
            let Some(path) = changed_file_path(payload) else {
                continue;
            };
            let path = project_relative_path(&payload.context.cwd, path);
            if !file_gates.iter().any(|gate| gate.file.is_match(&path)) {
                continue;
            }

            let project = self.project_mut(&payload.context.cwd);
            let id = project.next_change_id;
            project.next_change_id = project.next_change_id.saturating_add(1);
            project.file_changes.push(FileChangeRecord {
                id,
                path,
                timestamp: now,
            });
            project.prune_file_changes(now);
        }
    }

    /// Find the newest local file change that satisfies any of the supplied
    /// gates.
    pub fn latest_matching_file_change(
        &self,
        cwd: &Path,
        gates: &[FileChangeMatcher],
        now: u64,
    ) -> Option<FileChangeContext> {
        if gates.is_empty() {
            return None;
        }

        let project = self.projects.get(&project_key(cwd));
        project.and_then(|project| {
            project.file_changes.iter().rev().find_map(|change| {
                let age = now.saturating_sub(change.timestamp);
                if gates
                    .iter()
                    .any(|gate| gate.matches_change(&change.path, age))
                {
                    Some(FileChangeContext {
                        id: change.id,
                        path: change.path.clone(),
                        timestamp: change.timestamp,
                    })
                } else {
                    None
                }
            })
        })
    }

    /// Return reminder frequency state for a rule in the current project.
    pub fn reminder(&self, cwd: &Path, rule_name: &str) -> Option<&ReminderRecord> {
        self.projects
            .get(&project_key(cwd))
            .and_then(|project| project.reminders.get(rule_name))
    }

    /// Mark that a prompt reminder has been shown.
    pub fn mark_reminder_shown(
        &mut self,
        cwd: &Path,
        rule_name: &str,
        now: u64,
        change_id: Option<u64>,
        change_timestamp: Option<u64>,
    ) {
        let project = self.project_mut(cwd);
        project.reminders.insert(
            rule_name.to_string(),
            ReminderRecord {
                last_shown_at: now,
                last_change_id: change_id,
                last_change_timestamp: change_timestamp,
            },
        );
    }

    fn project_mut(&mut self, cwd: &Path) -> &mut ProjectInteractionState {
        self.projects.entry(project_key(cwd)).or_default()
    }
}

/// File-change context captured for prompt template interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangeContext {
    pub id: u64,
    pub path: String,
    pub timestamp: u64,
}

/// Frequency state for one reminder rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderRecord {
    pub last_shown_at: u64,

    #[serde(default)]
    pub last_change_id: Option<u64>,

    #[serde(default)]
    pub last_change_timestamp: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProjectInteractionState {
    #[serde(default)]
    next_change_id: u64,

    #[serde(default)]
    file_changes: Vec<FileChangeRecord>,

    #[serde(default)]
    reminders: BTreeMap<String, ReminderRecord>,
}

impl ProjectInteractionState {
    fn prune_file_changes(&mut self, now: u64) {
        self.file_changes
            .retain(|change| now.saturating_sub(change.timestamp) <= MAX_FILE_CHANGE_AGE_SECONDS);

        let excess = self
            .file_changes
            .len()
            .saturating_sub(MAX_FILE_CHANGES_PER_PROJECT);
        if excess > 0 {
            self.file_changes.drain(0..excess);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileChangeRecord {
    #[serde(default)]
    id: u64,

    path: String,
    timestamp: u64,
}

/// Whether any loaded rules need local interaction state.
pub fn rules_need_interaction_state(rules: &[Rule]) -> bool {
    rules.iter().any(Rule::uses_interaction_state)
}

/// Current Unix timestamp in seconds.
pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn changed_file_path(payload: &PreToolUse) -> Option<&Path> {
    match &payload.tool {
        ToolUse::Write(input) => Some(&input.file_path),
        ToolUse::Edit(input) => Some(&input.file_path),
        ToolUse::Delete(input) => Some(&input.file_path),
        ToolUse::WebFetch(_) | ToolUse::Bash(_) | ToolUse::Other { .. } => None,
    }
}

fn project_relative_path(cwd: &Path, path: &Path) -> String {
    let relative = if path.is_absolute() {
        path.strip_prefix(cwd).unwrap_or(path)
    } else {
        path
    };
    relative.to_string_lossy().replace('\\', "/")
}

fn project_key(cwd: &Path) -> String {
    cwd.canonicalize()
        .unwrap_or_else(|_| cwd.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn state_file_path() -> Result<Option<PathBuf>> {
    if let Ok(dir) = env::var("NUDGE_STATE_DIR") {
        return Ok(Some(PathBuf::from(dir).join(STATE_FILE_NAME)));
    }

    Ok(project_dirs().map(|dirs| dirs.data_local_dir().join(STATE_FILE_NAME)))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::TempDir;

    use crate::{
        agent::claude,
        hook::state::{InteractionState, now_seconds},
        rules::RuleConfig,
    };

    use super::*;

    fn rules(yaml: &str) -> Vec<Rule> {
        serde_yaml::from_str::<RuleConfig>(yaml)
            .expect("rules parse")
            .rules
    }

    #[test]
    fn records_only_opted_in_matching_file_changes() {
        let temp = TempDir::new().expect("temp dir");
        let hooks = claude::parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": temp.path(),
            "tool_name": "Write",
            "tool_input": { "file_path": "packages/hurry/src/main.rs", "content": "" }
        }))
        .expect("parse hook");
        let rules = rules(
            r#"
version: 1
rules:
  - name: hurry-test
    on:
      - hook: UserPromptSubmit
        intent:
          examples: ["try running it"]
        after_file_change:
          - file: "packages/hurry/src/**"
"#,
        );

        let now = now_seconds();
        let mut state = InteractionState::default();
        state.record_file_changes(&hooks, &rules, now);

        let change = state
            .latest_matching_file_change(
                temp.path(),
                &rules[0]
                    .hooks_userpromptsubmit()
                    .next()
                    .expect("matcher")
                    .after_file_change,
                now,
            )
            .expect("recorded change");
        assert_eq!(change.path, "packages/hurry/src/main.rs");
    }
}
