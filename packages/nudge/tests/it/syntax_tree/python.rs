//! Python syntax tree tests.

use std::fs;

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{edit_hook, run_hook_in_dir, run_nudge_in_dir, setup_config, write_hook};

#[test]
fn test_python_bare_except() {
    // Matches: bare except clauses (except: without specifying exception type)
    let config = r#"
version: 1
rules:
  - name: no-bare-except
    description: Don't use bare except clauses
    message: "Avoid bare `except:`. Specify the exception type (e.g., `except Exception:`)."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.py"
        content:
          - kind: SyntaxTree
            language: python
            query: "(except_clause !value) @exc"
"#;

    let dir = setup_config(config);

    // Should trigger: bare except
    let input = write_hook(
        "test.py",
        r#"try:
    risky_operation()
except:
    pass"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for bare except, got: {output}"
    );

    // Should pass: except with type
    let input = write_hook(
        "test.py",
        r#"try:
    risky_operation()
except Exception:
    pass"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for typed except, got: {output}"
    );
}

#[test]
fn test_python_print_function() {
    // Matches: print() calls (often left in for debugging)
    let config = r#"
version: 1
rules:
  - name: no-print
    description: Don't use print() in production code
    message: "Remove print() statement. Use logging instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.py"
        content:
          - kind: SyntaxTree
            language: python
            query: |
              (call
                function: (identifier) @fn
                (#eq? @fn "print"))
"#;

    let dir = setup_config(config);

    // Should trigger: print() call
    let input = write_hook("test.py", "print('debug info')");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for print(), got: {output}"
    );

    // Should pass: logging call
    let input = write_hook("test.py", "logger.info('debug info')");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for logging, got: {output}"
    );

    // Should pass: print text in comments and strings is not a call node.
    let input = write_hook(
        "test.py",
        r#"# print("debug")
message = "print('debug')""#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for comments and strings, got: {output}"
    );
}

#[test]
fn test_python_capture_and_suggestion_interpolation() {
    let config = r#"
version: 1
rules:
  - name: no-mutable-default
    description: Don't use mutable default arguments
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.py"
        content:
          - kind: SyntaxTree
            language: python
            query: |
              (function_definition
                name: (identifier) @fn
                parameters: (parameters
                  (default_parameter
                    name: (identifier) @arg
                    value: (list) @default)))
            suggestion: "Function `{{ $fn }}` uses mutable default `{{ $arg }}={{ $default }}`. Use `None` and initialize inside the body, then retry."
"#;

    let dir = setup_config(config);

    let input = write_hook(
        "cache.py",
        r#"def connect(cache=[]):
    return cache"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for mutable default, got: {output}"
    );
    assert!(
        output.contains("Function `connect` uses mutable default `cache=[]`"),
        "expected captured function, argument, and default in suggestion, got: {output}"
    );

    let input = write_hook(
        "cache.py",
        r#"def connect(cache=None):
    if cache is None:
        cache = []
    return cache"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for safe default, got: {output}"
    );
}

#[test]
fn test_python_edit_hook_uses_new_content() {
    let config = r#"
version: 1
rules:
  - name: no-print-edit
    description: Don't introduce print() in edits
    message: "Remove print() from the replacement text, then retry."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.py"
        new_content:
          - kind: SyntaxTree
            language: python
            query: |
              (call
                function: (identifier) @fn
                (#eq? @fn "print"))
"#;

    let dir = setup_config(config);

    let input = edit_hook("app.py", "logger.info('ready')", "print('ready')");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for print() introduced by Edit, got: {output}"
    );

    let input = edit_hook("app.py", "print('ready')", "logger.info('ready')");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough when Edit replacement is safe, got: {output}"
    );
}

#[test]
fn test_python_check_scans_files_and_interpolates_captures() {
    let config = r#"
version: 1
rules:
  - name: no-bare-except-check
    description: Don't use bare except clauses
    message: "Avoid bare `except:` in `{{ $exc }}`, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.py"
        content:
          - kind: SyntaxTree
            language: python
            query: "(except_clause !value) @exc"
"#;

    let dir = setup_config(config);
    fs::write(
        dir.path().join("handler.py"),
        r#"try:
    risky_operation()
except:
    pass
"#,
    )
    .expect("write Python fixture");
    fs::write(
        dir.path().join("safe.py"),
        r#"try:
    risky_operation()
except Exception:
    pass
"#,
    )
    .expect("write safe Python fixture");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check"]);
    assert_ne!(
        exit_code, 0,
        "expected nudge check to fail for bare except, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("handler.py:3 [no-bare-except-check]"),
        "expected check output to report handler.py line 3, got stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("Avoid bare `except:`"),
        "expected interpolated message in check output, got stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        !stdout.contains("safe.py"),
        "expected typed except file to pass, got stdout: {stdout}, stderr: {stderr}"
    );
}

#[test]
fn test_python_incomplete_code_uses_recovered_tree() {
    let config = r#"
version: 1
rules:
  - name: no-print-in-incomplete-code
    description: Don't use print(), even while code is incomplete
    message: "Remove print() from this Python code, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.py"
        content:
          - kind: SyntaxTree
            language: python
            query: |
              (call
                function: (identifier) @fn
                (#eq? @fn "print"))
"#;

    let dir = setup_config(config);

    let input = write_hook(
        "scratch.py",
        r#"print("debug")
def unfinished("#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected matcher to run on recovered Python tree, got: {output}"
    );
}
