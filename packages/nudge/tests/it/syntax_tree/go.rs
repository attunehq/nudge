//! Go syntax tree tests.

use std::fs;

use pretty_assertions::assert_eq as pretty_assert_eq;

use crate::edit_hook;

use super::{run_hook_in_dir, run_nudge_in_dir, setup_config, write_hook};

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

#[test]
fn test_go_capture_and_suggestion_interpolation() {
    let config = r#"
version: 1
rules:
  - name: no-go-panic
    description: Avoid panic in Go code
    message: "{{ $suggestion }}"
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
                (#eq? @fn "panic")) @call
            suggestion: "Avoid {{ $fn }} in Go code. Return an error instead."
"#;

    let dir = setup_config(config);
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
        output.contains("Avoid panic in Go code. Return an error instead."),
        "expected interpolated Go capture in suggestion, got: {output}"
    );
    assert!(
        output.contains("4 |     panic") && output.contains("^^^^^^^^^^^^^^^^^^^^^^^^^^^^^"),
        "expected snippet to use Go capture span, got: {output}"
    );
}

#[test]
fn test_go_edit_new_content_evaluation() {
    let config = r#"
version: 1
rules:
  - name: no-go-panic-edit
    description: Avoid panic in Go edits
    message: "Return an error instead of panicking."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.go"
        new_content:
          - kind: SyntaxTree
            language: go
            query: |
              (call_expression
                function: (identifier) @fn
                (#eq? @fn "panic"))
"#;

    let dir = setup_config(config);
    let input = edit_hook(
        "test.go",
        "return err",
        r#"func example() {
    panic("boom")
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for Go panic introduced by Edit, got: {output}"
    );
}

#[test]
fn test_go_check_scans_write_and_edit_rules() {
    let config = r#"
version: 1
rules:
  - name: no-go-panic-check-write
    description: Avoid panic in Go files
    message: "Return an error instead of panicking in {{ $fn }}."
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
  - name: no-empty-go-error-check
    description: Do not ignore Go errors
    message: "Handle this Go error check."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.go"
        new_content:
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
    fs::write(
        dir.path().join("bad.go"),
        r#"package main

func example() {
    panic("boom")
    if err != nil {}
}"#,
    )
    .expect("write Go file");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check"]);

    assert_ne!(exit_code, 0, "check should fail for Go issues");
    assert!(
        stdout.contains("bad.go:4 [no-go-panic-check-write]"),
        "expected Go Write SyntaxTree issue with line number, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("bad.go:5 [no-empty-go-error-check]"),
        "expected Go Edit SyntaxTree issue with line number, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("Return an error instead of panicking in panic."),
        "expected check message interpolation from Go capture, stdout: {stdout}"
    );
}

#[test]
fn test_go_comments_strings_and_safe_code_do_not_match() {
    let config = r#"
version: 1
rules:
  - name: no-go-panic
    description: Avoid panic in Go code
    message: "Return an error instead of panicking."
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
    let input = write_hook(
        "test.go",
        r#"package main

func example() error {
    // panic("comment only")
    msg := "panic(\"string only\")"
    defer cleanup()
    _ = msg
    return nil
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected comments, strings, defer, and safe returns to pass, got: {output}"
    );
}

#[test]
fn test_go_incomplete_code_uses_recovered_parse_tree() {
    let config = r#"
version: 1
rules:
  - name: no-go-panic-in-incomplete-code
    description: Avoid panic in incomplete Go code
    message: "Return an error instead of panicking."
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
    let input = write_hook(
        "test.go",
        r#"package main

func example() {
    panic("boom")
"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected recovered incomplete Go tree to match panic call, got: {output}"
    );
}
