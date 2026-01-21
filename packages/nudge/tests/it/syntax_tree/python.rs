//! Python syntax tree tests.

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

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
}
