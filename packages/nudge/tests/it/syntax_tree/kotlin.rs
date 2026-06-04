//! Kotlin syntax tree tests.

use std::fs;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;

use crate::{edit_hook, nudge_binary, run_nudge};

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

    // Should pass: logger call, plus println text in a comment and string
    let input = write_hook(
        "Test.kt",
        r#"fun main() {
    // println("debug")
    val text = "println(\"debug\")"
    logger.info(text)
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

#[test]
fn test_kotlin_edit_extension_function_suggestion_interpolation() {
    let config = r#"
version: 1
rules:
  - name: review-extension-function
    description: Flag Kotlin extension functions in generated edits
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.kt"
        new_content:
          - kind: SyntaxTree
            language: kotlin
            query: |
              (function_declaration
                (user_type (identifier) @receiver)
                name: (identifier) @fn_name)
            suggestion: "Review extension function `{{ $receiver }}.{{ $fn_name }}` before retrying."
"#;

    let dir = setup_config(config);

    let input = edit_hook(
        "User.kt",
        "old code",
        r#"fun User.displayName(): String {
    return name.orEmpty()
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for extension function, got: {output}"
    );
    assert!(
        output.contains("User.displayName"),
        "expected capture interpolation in suggestion, got: {output}"
    );

    let input = edit_hook(
        "User.kt",
        "old code",
        r#"fun displayName(user: User): String {
    return user.name.orEmpty()
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for ordinary function, got: {output}"
    );
}

#[test]
fn test_kotlin_check_scans_write_and_edit_rules() {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    fs::write(
        dir.path().join(".nudge.yaml"),
        r#"
version: 1
rules:
  - name: kotlin-data-class
    description: Review generated data classes
    message: "Kotlin data class `{{ $class_name }}` needs an explicit review."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.kt"
        content:
          - kind: SyntaxTree
            language: kotlin
            query: |
              (class_declaration
                (modifiers (class_modifier) @modifier)
                name: (identifier) @class_name
                (#eq? @modifier "data"))
  - name: kotlin-launch-call
    description: Review coroutine launch calls introduced by edits
    message: "Review coroutine launch on `{{ $receiver }}` before retrying."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.kt"
        new_content:
          - kind: SyntaxTree
            language: kotlin
            query: |
              (call_expression
                (navigation_expression
                  (identifier) @receiver
                  (identifier) @method)
                (#eq? @method "launch"))
"#,
    )
    .expect("write config");

    let src = dir.path().join("src");
    fs::create_dir(&src).expect("create src dir");
    fs::write(
        src.join("Unsafe.kt"),
        r#"data class User(val name: String?)
fun load(scope: CoroutineScope) {
    scope.launch { fetch() }
}"#,
    )
    .expect("write unsafe kotlin");
    fs::write(
        src.join("Safe.kt"),
        r#"class User(val name: String?)
fun load(scope: CoroutineScope) {
    // scope.launch { fetch() }
    val text = "scope.launch { fetch() }"
    scope.async { fetch() }
}"#,
    )
    .expect("write safe kotlin");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "src/Safe.kt"]);
    pretty_assert_eq!(
        exit_code,
        0,
        "safe Kotlin should pass check, stdout: {stdout}, stderr: {stderr}"
    );

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "src/Unsafe.kt"]);
    assert!(
        exit_code != 0,
        "unsafe Kotlin should fail check, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("src/Unsafe.kt:1 [kotlin-data-class]"),
        "expected data class issue at line 1, got: {stdout}"
    );
    assert!(
        stdout.contains("Kotlin data class `User` needs an explicit review."),
        "expected data class capture interpolation, got: {stdout}"
    );
    assert!(
        stdout.contains("src/Unsafe.kt:3 [kotlin-launch-call]"),
        "expected launch issue at line 3, got: {stdout}"
    );
    assert!(
        stdout.contains("Review coroutine launch on `scope` before retrying."),
        "expected launch capture interpolation, got: {stdout}"
    );
}

#[test]
fn test_kotlin_incomplete_code_uses_existing_parser_error_policy() {
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

    let input = write_hook(
        "Broken.kt",
        r#"fun main() {
    println("debug")
    val unfinished =
"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected complete Kotlin nodes to match despite nearby syntax errors, got: {output}"
    );

    let input = write_hook("Broken.kt", "{{{{");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected unmatched malformed Kotlin to pass silently, got: {output}"
    );
}

#[test]
fn test_kotlin_syntaxtree_cli() {
    let (exit_code, stdout, stderr) = run_nudge(&[
        "syntaxtree",
        "--language",
        "kotlin",
        "data class User(val name: String?)",
    ]);

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0, stderr: {stderr}");
    assert!(
        stdout.contains("class_declaration"),
        "syntaxtree should show Kotlin class_declaration node, got: {stdout}"
    );
    assert!(
        stdout.contains("nullable_type"),
        "syntaxtree should show Kotlin nullable_type node, got: {stdout}"
    );
    assert!(
        stdout.contains("User"),
        "syntaxtree should show Kotlin identifier text, got: {stdout}"
    );
}

fn run_nudge_in_dir(dir: &tempfile::TempDir, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(nudge_binary())
        .args(args)
        .current_dir(dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}
