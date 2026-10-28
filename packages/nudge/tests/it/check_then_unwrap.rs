//! Tests for the `prefer-let-else-over-check-unwrap` rule.
//!
//! The rule is a plain `SyntaxTree` query, so these tests load it from the
//! shipped config files instead of a copy. That keeps
//! `examples/rules/rust.yaml` and the dogfooded `.nudge.yaml` covered.

use std::fs;
use std::path::Path;
use std::process::Command;

use pretty_assertions::assert_eq as pretty_assert_eq;
use simple_test_case::test_case;
use tempfile::TempDir;

use crate::nudge_binary;

const RULE: &str = "prefer-let-else-over-check-unwrap";

/// Config files (relative to the repository root) that ship this rule.
const CONFIGS: [&str; 2] = ["examples/rules/rust.yaml", ".nudge.yaml"];

/// Run `nudge test` for the rule from `config` and return its stdout.
fn run_rule(config: &str, tool: &str, content: &str) -> String {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = TempDir::new().expect("create temp dir");
    fs::copy(repo_root.join(config), dir.path().join(".nudge.yaml")).expect("copy config");

    let output = Command::new(nudge_binary())
        .args([
            "test",
            "--rule",
            RULE,
            "--tool",
            tool,
            "--file",
            "src/lib.rs",
        ])
        .args(["--content", content])
        .current_dir(dir.path())
        .output()
        .expect("run nudge test");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    pretty_assert_eq!(
        output.status.code(),
        Some(0),
        "nudge test failed for {config}: {stdout}{stderr}"
    );
    stdout
}

#[test_case(
    "fn f(value: Option<u8>) -> u8 {\n    if value.is_none() {\n        return 0;\n    }\n    let value = value.unwrap();\n    value\n}",
    "let Some(..) = value else";
    "is_none then unwrap"
)]
#[test_case(
    "fn f(value: Option<u8>) -> Result<u8, E> {\n    if !value.is_some() {\n        return Err(E);\n    }\n    let value = value.unwrap();\n    Ok(value)\n}",
    "`if !value.is_some()`";
    "negated is_some then unwrap"
)]
#[test_case(
    "fn f(result: Result<u8, E>) -> u8 {\n    if result.is_err() {\n        return handle_error();\n    }\n    let value = result.unwrap();\n    value\n}",
    "`if result.is_err()`";
    "is_err then unwrap"
)]
#[test_case(
    "fn f(result: Result<u8, E>) -> u8 {\n    if !result.is_ok() { return 0; }\n    let value = result.expect(\"checked above\");\n    value\n}",
    "`.expect()`";
    "negated is_ok then expect"
)]
#[test_case(
    "fn f(&self) {\n    for _ in 0..3 {\n        if self.next.is_none() {\n            log();\n            continue;\n        }\n        let next = self.next.unwrap();\n    }\n}",
    "let Some(..) = self.next else";
    "field receiver with continue"
)]
#[test_case(
    "fn f(x: Option<u8>) {\n    loop {\n        if x.is_none() { break }\n        let x = x.unwrap();\n    }\n}",
    "`if x.is_none()`";
    "break without semicolon"
)]
#[test]
fn test_check_then_unwrap_interrupts(content: &str, expected_message: &str) {
    for config in CONFIGS {
        for tool in ["Write", "Edit"] {
            let stdout = run_rule(config, tool, content);
            assert!(
                stdout.contains("Result: Interrupt"),
                "expected interrupt from {config} ({tool}), got: {stdout}"
            );
            assert!(
                stdout.contains(expected_message),
                "expected `{expected_message}` from {config} ({tool}), got: {stdout}"
            );
        }
    }
}

#[test_case(
    "fn f(a: Option<u8>, b: Option<u8>) -> u8 {\n    if a.is_none() { return 0; }\n    let b = b.unwrap();\n    b\n}";
    "different value is unwrapped"
)]
#[test_case(
    "fn f(a: Option<u8>) -> u8 {\n    if a.is_none() { log(); }\n    let a = a.unwrap();\n    a\n}";
    "guard does not exit early"
)]
#[test_case(
    "fn f(a: Option<u8>) -> u8 {\n    if a.is_none() { if z { return 0; } log(); }\n    let a = a.unwrap();\n    a\n}";
    "early exit is not the last statement"
)]
#[test_case(
    "fn f(a: Option<u8>) -> u8 {\n    if a.is_none() { return 0; } else { log(); }\n    let a = a.unwrap();\n    a\n}";
    "guard has an else branch"
)]
#[test_case(
    "fn f(a: Option<u8>) -> u8 {\n    if a.is_none() { return 0; }\n    log();\n    let a = a.unwrap();\n    a\n}";
    "unwrap is not the next statement"
)]
#[test_case(
    "fn f(a: Option<u8>) -> u8 {\n    if a.is_none() || z { return 0; }\n    let a = a.unwrap();\n    a\n}";
    "compound condition"
)]
#[test_case(
    "fn f(a: Option<u8>) -> u8 {\n    if a.is_some() { return 0; }\n    let a = a.unwrap();\n    a\n}";
    "is_some without negation"
)]
#[test_case(
    "fn f(a: Option<u8>) -> u8 {\n    if a.is_none() { return 0; }\n    let a = a.unwrap_or(1);\n    a\n}";
    "unwrap_or is not an unwrap"
)]
#[test_case(
    "fn f(a: Option<u8>) -> u8 {\n    let Some(a) = a else { return 0; };\n    a\n}";
    "already let-else"
)]
#[test_case(
    "// if a.is_none() { return 0; }\n// let a = a.unwrap();\nfn f() -> &'static str {\n    \"if a.is_none() { return 0; }\\nlet a = a.unwrap();\"\n}";
    "comments and strings"
)]
#[test]
fn test_check_then_unwrap_passes(content: &str) {
    for config in CONFIGS {
        for tool in ["Write", "Edit"] {
            let stdout = run_rule(config, tool, content);
            assert!(
                stdout.contains("Result: Passthrough"),
                "expected passthrough from {config} ({tool}), got: {stdout}"
            );
        }
    }
}
