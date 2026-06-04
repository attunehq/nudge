//! CLI Subcommand Smoke Tests

use crate::run_nudge;
use pretty_assertions::assert_eq as pretty_assert_eq;

#[test]
fn test_validate_discovers_config() {
    // Should find .nudge.yaml in the project root
    let (exit_code, stdout, _stderr) = run_nudge(&["validate"]);

    pretty_assert_eq!(exit_code, 0, "validate should exit 0");
    // validate prints the parsed config as YAML
    assert!(
        stdout.contains(".nudge.yaml") || stdout.contains("no-inline-imports"),
        "validate should report found config, got: {stdout}"
    );
}

#[test]
fn test_validate_specific_file() {
    // Use CARGO_MANIFEST_DIR to get absolute path to the test config
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let config_path = format!("{manifest_dir}/.nudge.yaml");

    let (exit_code, stdout, _stderr) = run_nudge(&["validate", &config_path]);

    pretty_assert_eq!(exit_code, 0, "validate should exit 0");
    assert!(
        stdout.contains("no-inline-imports"),
        "validate should list rule names, got: {stdout}"
    );
}

#[test]
fn test_validate_nonexistent_file() {
    let (exit_code, stdout, _stderr) = run_nudge(&["validate", "nonexistent.yaml"]);

    pretty_assert_eq!(exit_code, 0, "validate should exit 0 for nonexistent file");
    // An empty list prints as [] in YAML
    assert!(
        stdout.contains("[]") || stdout.trim().is_empty(),
        "validate should report empty for nonexistent file, got: {stdout}"
    );
}

#[test]
fn test_test_rule_match() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "test",
        "--rule",
        "no-inline-imports",
        "--tool",
        "Write",
        "--file",
        "test.rs",
        "--content",
        "fn main() {\n    use std::io;\n}",
    ]);

    pretty_assert_eq!(exit_code, 0, "test command should exit 0");
    assert!(
        stdout.contains("Rule: no-inline-imports"),
        "test should show rule name, got: {stdout}"
    );
    assert!(
        stdout.contains("Result: Interrupt"),
        "test should show Interrupt for matching content, got: {stdout}"
    );
}

#[test]
fn test_test_rule_no_match() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "test",
        "--rule",
        "no-inline-imports",
        "--tool",
        "Write",
        "--file",
        "test.rs",
        "--content",
        "use std::io;\nfn main() {}",
    ]);

    pretty_assert_eq!(exit_code, 0, "test command should exit 0");
    assert!(
        stdout.contains("Result: Passthrough"),
        "test should show Passthrough for non-matching content, got: {stdout}"
    );
}

#[test]
fn test_test_rule_not_found() {
    let (exit_code, _stdout, stderr) = run_nudge(&[
        "test",
        "--rule",
        "nonexistent-rule",
        "--content",
        "anything",
    ]);

    // Should fail with an error
    assert!(exit_code != 0, "test should fail for nonexistent rule");
    assert!(
        stderr.contains("not found") || stderr.contains("nonexistent-rule"),
        "test should report rule not found, got: {stderr}"
    );
}

#[test]
fn test_syntaxtree_inline_code() {
    let (exit_code, stdout, _stderr) =
        run_nudge(&["syntaxtree", "--language", "rust", "fn main() {}"]);

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0");
    // Should show the function_item node
    assert!(
        stdout.contains("function_item"),
        "syntaxtree should show function_item node, got: {stdout}"
    );
    // Should show the identifier for 'main'
    assert!(
        stdout.contains("identifier") && stdout.contains("main"),
        "syntaxtree should show identifier 'main', got: {stdout}"
    );
}

#[test]
fn test_syntaxtree_typescript_language() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "syntaxtree",
        "--language",
        "typescript",
        "interface User { name: string; age?: number }",
    ]);

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0");
    assert!(
        stdout.contains("interface_declaration"),
        "syntaxtree should show TypeScript interface node, got: {stdout}"
    );
    assert!(
        stdout.contains("property_signature") && stdout.contains("age"),
        "syntaxtree should show TypeScript property signatures, got: {stdout}"
    );
}

#[test]
fn test_syntaxtree_typescript_legacy_alias() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "syntaxtree",
        "--language",
        "type-script",
        "const name = user.profile?.name;",
    ]);

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0");
    assert!(
        stdout.contains("member_expression"),
        "legacy type-script alias should still parse TypeScript, got: {stdout}"
    );
}

#[test]
fn test_syntaxtree_shows_field_names() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "syntaxtree",
        "--language",
        "rust",
        "fn foo() { let x = 1; }",
    ]);

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0");
    // Should show field names like 'name:' and 'body:'
    assert!(
        stdout.contains("name:"),
        "syntaxtree should show 'name:' field, got: {stdout}"
    );
    assert!(
        stdout.contains("body:"),
        "syntaxtree should show 'body:' field, got: {stdout}"
    );
}

#[test]
fn test_syntaxtree_go_inline_code() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "syntaxtree",
        "--language",
        "go",
        "package main\nfunc main() { defer cleanup() }",
    ]);

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0");
    assert!(
        stdout.contains("function_declaration"),
        "syntaxtree should show Go function_declaration node, got: {stdout}"
    );
    assert!(
        stdout.contains("defer_statement"),
        "syntaxtree should show Go defer_statement node, got: {stdout}"
    );
}

#[test]
fn test_syntaxtree_python_inline_code() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "syntaxtree",
        "--language",
        "python",
        "def greet(name):\n    print(name)",
    ]);

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0");
    assert!(
        stdout.contains("function_definition"),
        "syntaxtree should show function_definition node, got: {stdout}"
    );
    assert!(
        stdout.contains("call") && stdout.contains("print"),
        "syntaxtree should show print call nodes, got: {stdout}"
    );
}

#[test]
fn test_syntaxtree_accepts_csharp_language_name() {
    let (exit_code, stdout, _stderr) = run_nudge(&[
        "syntaxtree",
        "--language",
        "csharp",
        "public class Test { public void Example() { Console.WriteLine(\"debug\"); } }",
    ]);

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0");
    assert!(
        stdout.contains("class_declaration"),
        "syntaxtree should show C# class node, got: {stdout}"
    );
    assert!(
        stdout.contains("invocation_expression") && stdout.contains("WriteLine"),
        "syntaxtree should show C# invocation, got: {stdout}"
    );
}
