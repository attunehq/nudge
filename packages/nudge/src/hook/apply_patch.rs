//! Parser for Codex `apply_patch` tool input.

use std::{
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Result, bail};

use crate::hook::{DeleteInput, EditInput, ToolUse, WriteInput};

/// Parse an `apply_patch` command into normalized file tool uses.
pub fn parse(command: &str, cwd: &Path) -> Result<Vec<ToolUse>> {
    let lines = command.lines().collect::<Vec<_>>();
    if lines.first() != Some(&"*** Begin Patch") {
        bail!("missing apply_patch begin marker");
    }

    if !lines.iter().any(|line| *line == "*** End Patch") {
        bail!("missing apply_patch end marker");
    }

    let mut changes = Vec::new();
    let mut index = 1;
    while index < lines.len() {
        let line = lines[index];
        if line == "*** End Patch" {
            break;
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let (content, next) = parse_add_file(&lines, index + 1);
            changes.push(ToolUse::Write(WriteInput {
                file_path: path.into(),
                content,
            }));
            index = next;
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            changes.push(ToolUse::Delete(DeleteInput {
                file_path: path.into(),
            }));
            index += 1;
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let (update, next) = parse_update_file(&lines, index + 1);
            let current_path = cwd.join(path);
            let current = fs::read_to_string(&current_path)?;
            let new_string = apply_hunks(current, &update.hunks)?;
            changes.push(ToolUse::Edit(EditInput {
                file_path: update.move_to.unwrap_or_else(|| path.into()),
                old_string: update.old_string,
                new_string,
            }));
            index = next;
            continue;
        }

        bail!("unexpected apply_patch line: {line}");
    }

    Ok(changes)
}

struct ParsedUpdate {
    move_to: Option<PathBuf>,
    old_string: String,
    hunks: Vec<Hunk>,
}

#[derive(Debug)]
struct Hunk {
    old: String,
    new: String,
}

fn parse_add_file(lines: &[&str], mut index: usize) -> (String, usize) {
    let mut added = Vec::new();
    while index < lines.len() && !is_section_header(lines[index]) {
        if let Some(line) = lines[index].strip_prefix('+') {
            added.push(line.to_string());
        }
        index += 1;
    }

    (join_patch_lines(&added), index)
}

fn parse_update_file(lines: &[&str], mut index: usize) -> (ParsedUpdate, usize) {
    let mut move_to = None;
    let mut hunks = Vec::new();
    let mut old_lines = Vec::new();
    let mut new_lines = Vec::new();

    while index < lines.len() && !is_file_header(lines[index]) && lines[index] != "*** End Patch" {
        let line = lines[index];
        if let Some(path) = line.strip_prefix("*** Move to: ") {
            move_to = Some(path.into());
            index += 1;
            continue;
        }

        if line.starts_with("@@") {
            push_hunk(&mut hunks, &mut old_lines, &mut new_lines);
            index += 1;
            continue;
        }

        if let Some(content) = line.strip_prefix(' ') {
            old_lines.push(content.to_string());
            new_lines.push(content.to_string());
        } else if let Some(content) = line.strip_prefix('-') {
            old_lines.push(content.to_string());
        } else if let Some(content) = line.strip_prefix('+') {
            new_lines.push(content.to_string());
        }

        index += 1;
    }

    push_hunk(&mut hunks, &mut old_lines, &mut new_lines);
    let old_string = hunks
        .iter()
        .map(|hunk| hunk.old.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    (
        ParsedUpdate {
            move_to,
            old_string,
            hunks,
        },
        index,
    )
}

fn push_hunk(hunks: &mut Vec<Hunk>, old_lines: &mut Vec<String>, new_lines: &mut Vec<String>) {
    if old_lines.is_empty() && new_lines.is_empty() {
        return;
    }

    hunks.push(Hunk {
        old: join_patch_lines(old_lines),
        new: join_patch_lines(new_lines),
    });
    old_lines.clear();
    new_lines.clear();
}

fn apply_hunks(mut current: String, hunks: &[Hunk]) -> Result<String> {
    for hunk in hunks {
        if let Some(index) = current.find(&hunk.old) {
            current.replace_range(index..index + hunk.old.len(), &hunk.new);
            continue;
        }

        let old_without_final_newline = hunk.old.trim_end_matches('\n');
        if !old_without_final_newline.is_empty()
            && let Some(index) = current.find(old_without_final_newline)
        {
            current.replace_range(index..index + old_without_final_newline.len(), &hunk.new);
            continue;
        }

        bail!("apply_patch update hunk did not match current file");
    }

    Ok(current)
}

fn join_patch_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn is_section_header(line: &str) -> bool {
    is_file_header(line) || line == "*** End Patch"
}

fn is_file_header(line: &str) -> bool {
    line.starts_with("*** Add File: ")
        || line.starts_with("*** Update File: ")
        || line.starts_with("*** Delete File: ")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use crate::hook::{ToolUse, apply_patch};

    #[test]
    fn add_file_normalizes_to_write() {
        let parsed = apply_patch::parse(
            "*** Begin Patch\n*** Add File: src/main.rs\n+fn main() {}\n*** End Patch\n",
            Path::new("/tmp"),
        )
        .expect("parse patch");

        assert!(
            matches!(parsed.as_slice(), [ToolUse::Write(input)] if input.file_path == PathBuf::from("src/main.rs") && input.content == "fn main() {}\n")
        );
    }

    #[test]
    fn update_file_normalizes_to_edit_with_full_new_content() {
        let temp = TempDir::new().expect("temp dir");
        fs::write(temp.path().join("src.rs"), "fn main() {\n    old();\n}\n").expect("write file");

        let parsed = apply_patch::parse(
            "*** Begin Patch\n*** Update File: src.rs\n@@\n fn main() {\n-    old();\n+    new();\n }\n*** End Patch\n",
            temp.path(),
        )
        .expect("parse patch");

        assert!(
            matches!(parsed.as_slice(), [ToolUse::Edit(input)] if input.file_path == PathBuf::from("src.rs") && input.new_string == "fn main() {\n    new();\n}\n")
        );
    }

    #[test]
    fn delete_file_normalizes_to_delete() {
        let parsed = apply_patch::parse(
            "*** Begin Patch\n*** Delete File: old.rs\n*** End Patch\n",
            Path::new("/tmp"),
        )
        .expect("parse patch");

        assert!(
            matches!(parsed.as_slice(), [ToolUse::Delete(input)] if input.file_path == PathBuf::from("old.rs"))
        );
    }
}
