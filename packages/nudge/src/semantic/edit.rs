use std::{fs, ops::Range, path::Path};

use serde_json::Value;
use similar::{ChangeTag, TextDiff};

/// A verified proposed file and changed byte ranges in that file.
#[derive(Debug, Clone, PartialEq)]
pub struct EditSnapshot {
    pub content: String,
    pub changed: Vec<Range<usize>>,
}

impl EditSnapshot {
    pub fn between(before: &str, content: String) -> Self {
        let mut changed = Vec::new();
        let mut offset = 0;
        for change in TextDiff::from_lines(before, &content).iter_all_changes() {
            let end = offset + change.value().len();
            match change.tag() {
                ChangeTag::Equal => offset = end,
                ChangeTag::Insert => {
                    changed.push(offset..end);
                    offset = end;
                }
                ChangeTag::Delete => changed.push(offset..offset),
            }
        }
        Self { content, changed }
    }
}

/// Reconstruct only operations with unambiguous replacement semantics.
pub fn replacement(cwd: &Path, path: &Path, input: &Value) -> Option<EditSnapshot> {
    let before = fs::read_to_string(cwd.join(path)).ok()?;
    let mut after = before.clone();
    let edits = input
        .get("edits")
        .and_then(Value::as_array)
        .map(|edits| edits.iter().collect::<Vec<_>>())
        .unwrap_or_else(|| vec![input]);
    if edits.is_empty() {
        return None;
    }
    for edit in edits {
        let old = edit
            .get("old_string")
            .or_else(|| edit.get("oldString"))?
            .as_str()?;
        let new = edit
            .get("new_string")
            .or_else(|| edit.get("newString"))?
            .as_str()?;
        let replace_all = match edit.get("replace_all").or_else(|| edit.get("replaceAll")) {
            Some(value) => value.as_bool()?,
            None => false,
        };
        if old.is_empty() {
            return None;
        }
        let count = after.matches(old).count();
        if count == 0 || (!replace_all && count != 1) {
            return None;
        }
        after = if replace_all {
            after.replace(old, new)
        } else {
            after.replacen(old, new, 1)
        };
    }
    Some(EditSnapshot::between(&before, after))
}

/// Normalize a provider's sequential edits as one final file evaluation.
pub fn multi_input(cwd: &Path, path: &Path, input: &Value) -> crate::hook::EditInput {
    let snapshot = replacement(cwd, path, input);
    let strings = |name: &str, alias: &str| {
        input
            .get("edits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|edit| {
                edit.get(name)
                    .or_else(|| edit.get(alias))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    crate::hook::EditInput {
        file_path: path.to_path_buf(),
        old_string: strings("old_string", "oldString"),
        new_string: strings("new_string", "newString"),
        post_edit_content: snapshot.as_ref().map(|snapshot| snapshot.content.clone()),
        semantic_snapshot: snapshot,
    }
}

pub fn intersects(dependency: &Range<usize>, change: &Range<usize>) -> bool {
    if change.is_empty() {
        dependency.start <= change.start && change.start <= dependency.end
    } else {
        dependency.start < change.end && change.start < dependency.end
    }
}
