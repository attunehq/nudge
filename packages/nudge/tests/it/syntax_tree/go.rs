//! Go syntax tree tests.

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

#[test]
fn test_go_empty_error_check() {
    // Matches: if err != nil with empty body
    let config = r#"
version: 1
rules:
  - name: no-empty-error-check
    description: Don't ignore errors with empty if blocks
    message: "Don't ignore errors. Handle the error or explicitly return/log it."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.go"
        content:
          - kind: SyntaxTree
            language: go
            query: |
              (if_statement
                condition: (binary_expression
                  left: (identifier) @err
                  right: (nil))
                consequence: (block) @body
                (#eq? @err "err")
                (#eq? @body "{}"))
"#;

    let dir = setup_config(config);

    // Should trigger: empty error check
    let input = write_hook(
        "test.go",
        r#"package main

func example() {
    _, err := doSomething()
    if err != nil {}
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for empty error check, got: {output}"
    );

    // Should pass: error check with return
    let input = write_hook(
        "test.go",
        r#"package main

func example() error {
    _, err := doSomething()
    if err != nil {
        return err
    }
    return nil
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for proper error handling, got: {output}"
    );
}

#[test]
fn test_go_panic_call() {
    // Matches: panic() calls
    let config = r#"
version: 1
rules:
  - name: no-panic
    description: Avoid using panic in library code
    message: "Avoid `panic()`. Return an error instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.go"
        content:
          - kind: SyntaxTree
            language: go
            query: |
              (call_expression
                function: (identifier) @fn
                (#eq? @fn "panic"))
"#;

    let dir = setup_config(config);

    // Should trigger: panic call
    let input = write_hook(
        "test.go",
        r#"package main

func example() {
    panic("something went wrong")
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for panic, got: {output}"
    );

    // Should pass: returning error
    let input = write_hook(
        "test.go",
        r#"package main

import "errors"

func example() error {
    return errors.New("something went wrong")
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for error return, got: {output}"
    );
}
