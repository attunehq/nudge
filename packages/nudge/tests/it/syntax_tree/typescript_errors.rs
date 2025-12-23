//! TypeScript error handling tests (Category 4).

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

#[test]
fn test_typescript_empty_catch_block() {
    // Matches: catch blocks with empty body
    let config = r#"
version: 1
rules:
  - name: no-empty-catch
    description: Catch blocks should not be empty
    message: "Empty catch block. At minimum, log the error."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (catch_clause
                body: (statement_block) @body
                (#eq? @body "{}"))
"#;

    let dir = setup_config(config);

    // Should trigger: empty catch block
    let input = write_hook(
        "test.ts",
        r#"try {
    riskyOperation();
} catch (e) {}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for empty catch, got: {output}"
    );

    // Should pass: catch with error handling
    let input = write_hook(
        "test.ts",
        r#"try {
    riskyOperation();
} catch (e) {
    console.error(e);
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for non-empty catch, got: {output}"
    );
}

#[test]
fn test_typescript_throw_string_literal() {
    // Matches: throw with string literal instead of Error
    let config = r#"
version: 1
rules:
  - name: throw-error-objects
    description: Throw Error objects, not primitives
    message: "Throw an Error object instead of a string literal."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(throw_statement (string) @thrown)"
"#;

    let dir = setup_config(config);

    // Should trigger: throwing string literal
    let input = write_hook("test.ts", "throw 'Something went wrong';");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for throw string, got: {output}"
    );

    // Should pass: throwing Error object
    let input = write_hook("test.ts", "throw new Error('Something went wrong');");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for Error object, got: {output}"
    );
}

#[test]
fn test_typescript_nested_try_blocks() {
    // Matches: try blocks nested inside try blocks
    let config = r#"
version: 1
rules:
  - name: no-nested-try
    description: Avoid nested try blocks
    message: "Nested try blocks are hard to reason about. Refactor into separate functions."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (try_statement
                body: (statement_block
                  (try_statement) @nested))
"#;

    let dir = setup_config(config);

    // Should trigger: nested try blocks
    let input = write_hook(
        "test.ts",
        r#"try {
    try {
        dangerousOp();
    } catch (inner) {}
} catch (outer) {}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for nested try, got: {output}"
    );

    // Should pass: single try block
    let input = write_hook(
        "test.ts",
        r#"try {
    dangerousOp();
} catch (e) {
    handleError(e);
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for single try, got: {output}"
    );
}

#[test]
fn test_typescript_promise_reject_non_error() {
    // Matches: Promise.reject with non-Error argument
    let config = r#"
version: 1
rules:
  - name: reject-with-error
    description: Use Error objects with Promise.reject
    message: "Pass an Error object to Promise.reject for proper stack traces."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (call_expression
                function: (member_expression
                  object: (identifier) @obj
                  property: (property_identifier) @prop)
                arguments: (arguments (string) @arg)
                (#eq? @obj "Promise")
                (#eq? @prop "reject"))
"#;

    let dir = setup_config(config);

    // Should trigger: Promise.reject with string
    let input = write_hook("test.ts", "return Promise.reject('Failed to load');");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for reject string, got: {output}"
    );

    // Should pass: Promise.reject with Error
    let input = write_hook("test.ts", "return Promise.reject(new Error('Failed to load'));");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for reject Error, got: {output}"
    );
}
