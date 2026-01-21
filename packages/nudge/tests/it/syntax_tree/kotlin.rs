//! Kotlin syntax tree tests.

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

#[test]
fn test_kotlin_println() {
    // Matches: println() calls
    let config = r#"
version: 1
rules:
  - name: no-println
    description: Don't use println in production code
    message: "Use a logging framework instead of `println`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.kt"
        content:
          - kind: SyntaxTree
            language: kotlin
            query: |
              (call_expression
                (identifier) @fn
                (#eq? @fn "println"))
"#;

    let dir = setup_config(config);

    // Should trigger: println call
    let input = write_hook(
        "Test.kt",
        r#"fun main() {
    println("debug")
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for println, got: {output}"
    );

    // Should pass: logger call
    let input = write_hook(
        "Test.kt",
        r#"fun main() {
    logger.info("debug")
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for logger, got: {output}"
    );
}

#[test]
fn test_kotlin_bang_bang_operator() {
    // Matches: !! (not-null assertion) operator
    let config = r#"
version: 1
rules:
  - name: no-bang-bang
    description: Avoid using !! operator
    message: "Avoid `!!`. Use safe calls (`?.`) or explicit null checks instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.kt"
        content:
          - kind: SyntaxTree
            language: kotlin
            query: "(unary_expression operator: (\"!!\")) @assertion"
"#;

    let dir = setup_config(config);

    // Should trigger: !! operator
    let input = write_hook(
        "Test.kt",
        r#"fun example(name: String?) {
    val len = name!!.length
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for !!, got: {output}"
    );

    // Should pass: safe call
    let input = write_hook(
        "Test.kt",
        r#"fun example(name: String?) {
    val len = name?.length
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for safe call, got: {output}"
    );
}
