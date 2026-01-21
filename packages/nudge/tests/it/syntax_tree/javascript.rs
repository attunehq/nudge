//! JavaScript syntax tree tests.

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

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
