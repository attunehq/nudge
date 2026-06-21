//! TypeScript end-to-end SyntaxTree coverage.

use std::fs;

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{edit_hook, run_hook_in_dir, run_nudge_in_dir, setup_config, write_hook};

#[test]
fn test_typescript_write_interpolates_captures_and_suggestions() {
    let config = r#"
version: 1
rules:
  - name: no-any-parameters
    description: Avoid any in TypeScript parameters
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (required_parameter
                pattern: (identifier) @param
                type: (type_annotation (predefined_type) @type)
                (#eq? @type "any"))
            suggestion: "Parameter `{{ $param }}` uses `{{ $type }}`."
"#;

    let dir = setup_config(config);

    let input = write_hook("src/service.ts", "function process(data: any): void {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for any parameter, got: {output}"
    );
    assert!(
        output.contains("Parameter `data` uses `any`."),
        "expected TypeScript capture interpolation in suggestion, got: {output}"
    );

    let safe_input = write_hook(
        "src/service.ts",
        r#"
// function process(data: any): void {}
const sample = "function process(data: any): void {}";
function process(data: unknown): void {}
"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &safe_input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected comments, strings, and unknown type to pass, got: {output}"
    );

    let js_input = write_hook("src/service.js", "function process(data: any): void {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &js_input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected .js files to pass a .ts TypeScript rule, got: {output}"
    );
}

#[test]
fn test_typescript_edit_evaluates_new_content_with_non_null_assertions() {
    let config = r#"
version: 1
rules:
  - name: no-non-null-assertions
    description: Avoid TypeScript non-null assertions
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.ts"
        new_content:
          - kind: SyntaxTree
            language: typescript
            query: "(non_null_expression) @assertion"
            suggestion: "Replace `{{ $assertion }}` with optional chaining or a guard."
"#;

    let dir = setup_config(config);

    let input = edit_hook(
        "src/user.ts",
        "const name = user.profile?.name;",
        "const name = user.profile!.name;",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for non-null assertion edit, got: {output}"
    );
    assert!(
        output.contains("user.profile!"),
        "expected captured non-null expression text in suggestion, got: {output}"
    );

    let safe_input = edit_hook(
        "src/user.ts",
        "const name = user.profile!.name;",
        "const name = user.profile?.name;",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &safe_input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected optional chaining edit to pass, got: {output}"
    );
}

#[test]
fn test_typescript_check_scans_files_and_reports_capture_lines() {
    let config = r#"
version: 1
rules:
  - name: no-exported-interfaces
    description: Prefer type aliases in this project
    message: "Convert interface `{{ $iface }}` to a type alias."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(interface_declaration name: (type_identifier) @iface)"
"#;

    let dir = setup_config(config);
    let src_dir = dir.path().join("src");
    fs::create_dir(&src_dir).expect("create src dir");
    fs::write(
        src_dir.join("user.ts"),
        "\nexport interface User {\n  name: string;\n}\n",
    )
    .expect("write TypeScript fixture");
    fs::write(src_dir.join("user.js"), "export interface User {}\n")
        .expect("write JavaScript fixture");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "src"]);
    let output = format!("{stdout}{stderr}");

    pretty_assert_eq!(exit_code, 1, "expected check to fail, output: {output}");
    assert!(
        output.contains("src/user.ts:2 [no-exported-interfaces]")
            || output.contains("src\\user.ts:2 [no-exported-interfaces]"),
        "expected TypeScript issue on the interface line, got: {output}"
    );
    assert!(
        output.contains("Convert interface `User` to a type alias."),
        "expected interface capture interpolation, got: {output}"
    );
    assert!(
        !output.contains("src/user.js"),
        "expected .js fixture to be ignored by the .ts rule, got: {output}"
    );
}

#[test]
fn test_typescript_incomplete_code_uses_error_tolerant_parse_tree() {
    let config = r#"
version: 1
rules:
  - name: no-any-parameters
    description: Avoid any in TypeScript parameters
    message: "Parameter `{{ $param }}` uses `{{ $type }}`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (required_parameter
                pattern: (identifier) @param
                type: (type_annotation (predefined_type) @type)
                (#eq? @type "any"))
"#;

    let dir = setup_config(config);

    let input = write_hook("src/service.ts", "function process(data: any");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected incomplete TypeScript to still expose parsed captures, got: {output}"
    );
    assert!(
        output.contains("Parameter `data` uses `any`."),
        "expected captures from the error-tolerant parse tree, got: {output}"
    );

    let invalid_input = write_hook("src/service.ts", "{{{{");
    let (exit_code, output) = run_hook_in_dir(&dir, &invalid_input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected unrelated invalid TypeScript to pass without matching, got: {output}"
    );
}
