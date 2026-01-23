//! Build script for nudge that generates version information.
//!
//! This generates a version string that:
//! - Uses `git describe --always` to get the base version (tag or commit hash)
//! - If the working tree is dirty, appends a content hash of the changed files

use std::fs;
use std::hash::{DefaultHasher, Hasher as _};
use std::iter;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

fn main() -> Result<(), String> {
    let version = compute_version()?;
    println!("cargo:rustc-env=NUDGE_VERSION={version}");

    Ok(())
}

fn compute_version() -> Result<String, String> {
    let base_version = git_describe()?;
    let changed_files = changed_files()?;

    if changed_files.is_empty() {
        return Ok(base_version);
    }

    let content_hash = content_hash(changed_files)?;
    let short_hash = &content_hash[..7.min(content_hash.len())];

    Ok(format!("{base_version}-{short_hash}"))
}

fn content_hash(mut files: Vec<StatusEntry>) -> Result<String, String> {
    files.sort();
    files.dedup();

    let repo_root = repo_root()?;

    let mut hashes = Vec::new();
    for file in files {
        let path = Path::new(&repo_root).join(file.path);
        let mut hasher = DefaultHasher::new();
        #[allow(clippy::disallowed_methods)]
        if let Ok(content) = fs::read(&path) {
            hasher.write(path.as_os_str().as_encoded_bytes());
            hasher.write(&content);
            let hash = hasher.finish();
            hashes.push(hash);
        }
    }
    hashes.sort();
    hashes.dedup();

    let mut hasher = DefaultHasher::new();
    for hash in hashes {
        hasher.write_u64(hash);
    }
    let final_hash = hasher.finish();

    Ok(format!("{final_hash:x}"))
}

fn run(prog: &str, argv: &[&str]) -> Result<String, String> {
    let invocation = iter::once(prog)
        .chain(argv.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");

    let output = Command::new(prog)
        .args(argv)
        .output()
        .map_err(|e| format!("failed to execute `{invocation}`: {e}"))?;
    if !output.status.success() {
        return Err(format!("`{invocation}` exited with non-zero status"));
    }

    let output = String::from_utf8(output.stdout)
        .map_err(|e| format!("could not parse output of `{invocation}` as UTF-8: {e}"))?;
    Ok(output.trim_end().to_string())
}

fn git_describe() -> Result<String, String> {
    run("git", &["describe", "--always", "--tags", "--dirty=-dirty"])
}

fn repo_root() -> Result<String, String> {
    run("git", &["rev-parse", "--show-toplevel"])
}

fn changed_files() -> Result<Vec<StatusEntry>, String> {
    let output = run("git", &["status", "--porcelain"])?;

    let mut files = Vec::new();
    for line in output.lines() {
        files.push(line.parse::<StatusEntry>()?);
    }

    Ok(files)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum GitFileStatus {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
    Untracked,
    Ignored,
}

impl GitFileStatus {
    fn parse(c: char) -> Option<Self> {
        match c {
            ' ' => Some(Self::Unmodified),
            'M' => Some(Self::Modified),
            'A' => Some(Self::Added),
            'D' => Some(Self::Deleted),
            'R' => Some(Self::Renamed),
            'C' => Some(Self::Copied),
            'U' => Some(Self::Unmerged),
            '?' => Some(Self::Untracked),
            '!' => Some(Self::Ignored),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StatusEntry {
    index: GitFileStatus,
    worktree: GitFileStatus,
    path: String,
    orig_path: Option<String>,
}

impl FromStr for StatusEntry {
    type Err = String;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        if line.len() < 4 {
            return Err("line too short".into());
        }

        let mut chars = line.chars();
        let index_char = chars.next().unwrap();
        let worktree_char = chars.next().unwrap();
        let space = chars.next().unwrap();

        if space != ' ' {
            return Err("expected space after status".into());
        }

        let index = GitFileStatus::parse(index_char)
            .ok_or_else(|| format!("invalid index status: {index_char}"))?;
        let worktree = GitFileStatus::parse(worktree_char)
            .ok_or_else(|| format!("invalid worktree status: {worktree_char}"))?;

        let rest = chars.collect::<String>();
        let (path, orig_path) = if matches!(index, GitFileStatus::Renamed | GitFileStatus::Copied) {
            if let Some((old, new)) = rest.split_once(" -> ") {
                (new.to_string(), Some(old.to_string()))
            } else {
                (rest, None)
            }
        } else {
            (rest, None)
        };

        Ok(StatusEntry {
            index,
            worktree,
            path,
            orig_path,
        })
    }
}
