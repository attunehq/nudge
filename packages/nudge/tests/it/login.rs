use std::{io::Write, process::Stdio};

use tempfile::TempDir;

use crate::isolated_command;

#[test]
fn login_requires_explicit_stdin_and_rejects_empty_or_multiline_keys_offline() {
    let home = TempDir::new().expect("temp");
    let output = isolated_command(home.path())
        .args(["login", "typesafe.ai"])
        .stdin(Stdio::null())
        .output()
        .expect("login");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--stdin"));
    for input in ["", " \n", "test-key-ada\ntest-key-grace\n"] {
        let mut child = isolated_command(home.path())
            .args(["login", "typesafe.ai", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("login");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("key");
        let output = child.wait_with_output().expect("result");
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("non-empty ASCII without whitespace"));
        assert!(!stderr.contains("test-key-ada"));
        assert!(output.stdout.is_empty());
    }
}
