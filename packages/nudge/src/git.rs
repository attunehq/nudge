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
    use pretty_assertions::assert_eq as pretty_assert_eq;
    use std::process::Command;

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("run git");

        assert!(
            output.status.success(),
            "git command failed: git -C {} {}\nstdout:\n{}\nstderr:\n{}",
            cwd.display(),
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn test_current_branch_in_git_repo() {
        let dir = tempfile::TempDir::new().expect("create temp dir");
        git(dir.path(), &["init"]);
        git(dir.path(), &["checkout", "-b", "nudge-test-branch"]);

        let branch = current_branch(dir.path());

        pretty_assert_eq!(branch.as_deref(), Some("nudge-test-branch"));
    }

    #[test]
    fn test_current_branch_not_git_repo() {
        let dir = tempfile::TempDir::new().expect("create temp dir");

        let branch = current_branch(dir.path());

        assert!(branch.is_none(), "expected None for non-git directory");
    }
}
