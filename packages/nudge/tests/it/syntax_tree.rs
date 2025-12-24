//! Integration tests for SyntaxTree matcher.
//!
//! These tests verify that tree-sitter based matching works through the full
//! hook pipeline, including correct handling of AST-based patterns that regex
//! cannot express.

mod typescript_errors;
mod typescript_types;

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

/// Create a temporary directory with a .nudge.yaml config containing the given rules.
fn setup_config(rules_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join(".nudge.yaml");
    std::fs::write(&config_path, rules_yaml).expect("write config");
    dir
}

/// Get the path to the built nudge binary.
fn get_binary_path() -> PathBuf {
    // Build the binary
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "nudge"])
        .status()
        .expect("failed to build nudge");
    assert!(status.success(), "cargo build failed");

    // Get the target directory - use CARGO_TARGET_DIR or default
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    workspace_root.join("target/debug/nudge")
}

/// Run nudge claude hook with the given input JSON in the specified directory.
fn run_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let binary = get_binary_path();

    // Run the binary directly with the temp dir as cwd
    let mut child = Command::new(&binary)
        .args(["claude", "hook"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn nudge");

    {
        let stdin = child.stdin.as_mut().expect("failed to get stdin");
        stdin
            .write_all(input.as_bytes())
            .expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait for nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let combined = if !stdout.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };

    (exit_code, combined)
}

/// Build a PreToolUse hook JSON payload for Write tool.
fn write_hook(file_path: &str, content: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "tool_name": "Write",
        "tool_use_id": "123",
        "tool_input": {
            "file_path": file_path,
            "content": content
        }
    })
    .to_string()
}

#[test]
fn test_syntax_tree_matches_use_in_function_body() {
    let config = r#"
version: 1
rules:
  - name: no-inline-imports-ast
    description: Move imports to the top of the file (AST-based)
    message: "Move this `use` to the top of the file, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: "(function_item body: (block (use_declaration) @use))"
"#;

    let dir = setup_config(config);

    // This should trigger: use inside function body
    let input = write_hook("test.rs", "fn main() {\n    use std::io;\n}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for use inside function, got: {output}"
    );
}

#[test]
fn test_syntax_tree_passes_top_level_use() {
    let config = r#"
version: 1
rules:
  - name: no-inline-imports-ast
    description: Move imports to the top of the file (AST-based)
    message: "Move this `use` to the top of the file, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: "(function_item body: (block (use_declaration) @use))"
"#;

    let dir = setup_config(config);

    // This should pass: use at top level, not inside function
    let input = write_hook("test.rs", "use std::io;\n\nfn main() {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for top-level use, got: {output}"
    );
}

#[test]
fn test_syntax_tree_passes_use_in_mod_test() {
    // This is the key advantage over regex: we can distinguish use in function
    // bodies from use in mod test blocks.
    let config = r#"
version: 1
rules:
  - name: no-inline-imports-ast
    description: Move imports to the top of the file (AST-based)
    message: "Move this `use` to the top of the file, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: "(function_item body: (block (use_declaration) @use))"
"#;

    let dir = setup_config(config);

    // This should pass: use inside mod test (not a function body)
    let input = write_hook(
        "test.rs",
        r#"
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {}
}
"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for use in mod test, got: {output}"
    );
}

#[test]
fn test_syntax_tree_with_suggestion_interpolation() {
    let config = r#"
version: 1
rules:
  - name: function-name-check
    description: Check function naming
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: "(function_item name: (identifier) @fn_name)"
            suggestion: "Found function named `{{ $fn_name }}`"
"#;

    let dir = setup_config(config);

    let input = write_hook("test.rs", "fn my_test_function() {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains("my_test_function"),
        "expected suggestion to contain function name, got: {output}"
    );
}
