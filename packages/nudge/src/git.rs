//! Git state queries via shell commands.

use std::path::Path;
use std::process::Command;

/// Get the current git branch name.
///
/// Returns `None` if:
/// - Not in a git repository
/// - Git command fails
/// - In detached HEAD state (no branch name)
pub fn current_branch(cwd: &Path) -> Option<String> {
    let cwd_str = cwd.to_str()?;

    let output = Command::new("git")
        .args(["-C", cwd_str, "branch", "--show-current"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_current_branch_in_git_repo() {
        // This test runs in the nudge repo, so we should get a branch name
        let cwd = env::current_dir().expect("get cwd");
        let branch = current_branch(&cwd);
        // We should be on some branch (could be main, a feature branch, etc.)
        assert!(branch.is_some(), "expected to be in a git repo with a branch");
    }

    #[test]
    fn test_current_branch_not_git_repo() {
        // /tmp is unlikely to be a git repo
        let branch = current_branch(Path::new("/tmp"));
        assert!(branch.is_none(), "expected None for non-git directory");
    }
}
