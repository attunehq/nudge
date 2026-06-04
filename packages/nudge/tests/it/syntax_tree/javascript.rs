//! JavaScript syntax tree tests.

use std::fs;

use crate::edit_hook;
use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, run_nudge_in_dir, setup_config, write_hook};

#[test]
fn test_javascript_console_log() {
    // Matches: console.log() calls
    let config = r#"
version: 1
rules:
  - name: no-console-log
    description: Don't use console.log in production code
    message: "Remove console.log statement."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.js"
        content:
          - kind: SyntaxTree
            language: javascript
            query: |
              (call_expression
                function: (member_expression
                  object: (identifier) @obj
                  property: (property_identifier) @prop)
                (#eq? @obj "console")
                (#eq? @prop "log"))
"#;

    let dir = setup_config(config);

    // Should trigger: console.log
    let input = write_hook("test.js", "console.log('hello');");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for console.log, got: {output}"
    );

    // Should pass: console.error (different method)
    let input = write_hook("test.js", "console.error('error');");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for console.error, got: {output}"
    );

    // Should pass: console.log inside a string or comment is not a call
    let input = write_hook(
        "test.js",
        r#"const text = "console.log('hello')";
// console.log('hello');
"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for text-only console.log, got: {output}"
    );
}

#[test]
fn test_javascript_var_declaration() {
    // Matches: var declarations (prefer let/const)
    let config = r#"
version: 1
rules:
  - name: no-var
    description: Don't use var, use let or const instead
    message: "Use `let` or `const` instead of `var`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.js"
        content:
          - kind: SyntaxTree
            language: javascript
            query: "(variable_declaration) @var"
"#;

    let dir = setup_config(config);

    // Should trigger: var declaration
    let input = write_hook("test.js", "var x = 1;");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for var, got: {output}"
    );

    // Should pass: let declaration
    let input = write_hook("test.js", "let x = 1;");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for let, got: {output}"
    );
}

#[test]
fn test_javascript_loose_equality_captures_and_suggestion() {
    let config = javascript_loose_equality_config();
    let dir = setup_config(config);

    let input = write_hook(
        "test.js",
        "if (user == null) {\n    handleMissingUser();\n}",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for loose equality, got: {output}"
    );
    assert!(
        output.contains("Use strict equality: user === null."),
        "expected interpolated JavaScript capture suggestion, got: {output}"
    );

    for safe_code in [
        "if (user === null) {\n    handleMissingUser();\n}",
        "const example = \"user == null\";",
        "// if (user == null) {}\nconst user = null;",
    ] {
        let input = write_hook("test.js", safe_code);
        let (exit_code, output) = run_hook_in_dir(&dir, &input);
        pretty_assert_eq!(exit_code, 0, "expected exit 0");
        assert!(
            output.is_empty(),
            "expected passthrough for safe JavaScript, got: {output}"
        );
    }
}

#[test]
fn test_javascript_eval_call_matches_edit_new_content_only() {
    let config = r#"
version: 1
rules:
  - name: no-eval-js-edit
    description: Don't introduce eval() in JavaScript edits
    message: "Avoid eval() in JavaScript edits."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.js"
        new_content:
          - kind: SyntaxTree
            language: javascript
            query: |
              (call_expression
                function: (identifier) @fn
                (#eq? @fn "eval")) @call
"#;

    let dir = setup_config(config);

    let input = edit_hook(
        "test.js",
        "const value = input;",
        "const value = eval(input);",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for introduced eval, got: {output}"
    );

    let input = edit_hook(
        "test.js",
        "const value = eval(input);",
        "const value = JSON.parse(input);",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough when new content removes eval, got: {output}"
    );
}

#[test]
fn test_javascript_syntax_tree_check_scans_js_files() {
    let config = javascript_loose_equality_config();
    let dir = setup_config(config);
    let src = dir.path().join("src");
    fs::create_dir(&src).expect("create src directory");
    fs::write(
        src.join("bad.js"),
        "const status = 'ready';\nif (user == null) {\n    loadUser();\n}\n",
    )
    .expect("write bad JavaScript fixture");
    fs::write(
        src.join("safe.js"),
        "const example = \"user == null\";\nif (user === null) {\n    loadUser();\n}\n",
    )
    .expect("write safe JavaScript fixture");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "src"]);
    pretty_assert_eq!(
        exit_code,
        1,
        "expected check to report JavaScript issue, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("src/bad.js:2 [no-loose-equality-js]"),
        "expected check to report bad.js line 2, got: {stdout}"
    );
    assert!(
        stdout.contains("Use strict equality: user === null."),
        "expected interpolated check message, got: {stdout}"
    );
    assert!(
        !stdout.contains("safe.js"),
        "expected check to skip safe JavaScript file, got: {stdout}"
    );
}

fn javascript_loose_equality_config() -> &'static str {
    r#"
version: 1
rules:
  - name: no-loose-equality-js
    description: Use strict equality in JavaScript
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.js"
        content:
          - kind: SyntaxTree
            language: javascript
            query: |
              (binary_expression
                left: (_) @left
                operator: "==" @operator
                right: (_) @right) @comparison
            suggestion: "Use strict equality: {{ $left }} === {{ $right }}."
"#
}
