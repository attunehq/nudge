//! Tests for the `prefer-find-over-search-loop` rule.
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

const RULE: &str = "prefer-find-over-search-loop";

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
    "fn f(items: Vec<u8>) -> Option<u8> {\n    let mut found = None;\n    for item in items {\n        if item > 3 {\n            found = Some(item);\n            break;\n        }\n    }\n    found\n}",
    "Set `found` with an iterator search over `items`";
    "find"
)]
#[test_case(
    "fn f(items: &[u8]) -> bool {\n    let mut big = false;\n    for x in items.iter() { if *x > 3 { big = true; break } }\n    big\n}",
    "Set `big` with an iterator search over `items.iter()`";
    "any with break without semicolon"
)]
#[test_case(
    "fn f(keys: &[&str]) {\n    let mut value: Option<String> = None;\n    for key in keys {\n        if let Ok(v) = env::var(key) {\n            value = Some(v);\n            break;\n        }\n    }\n}",
    "Set `value` with an iterator search over `keys`";
    "find_map with if let and a type annotation"
)]
#[test_case(
    "fn f(items: &[u8]) {\n    let mut index = None;\n    for (i, item) in items.iter().enumerate() {\n        if *item == 0 { index = Some(i); break; }\n    }\n}",
    "`.position(..)` for an index";
    "position over enumerate"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut found = None;\n    // Look for the first big one.\n    for item in items {\n        // Big enough?\n        if item > 3 {\n            found = Some(item); // keep it\n            /* done */\n            break;\n        }\n        // otherwise keep going\n    }\n}",
    "Set `found`";
    "comments around the loop and the assignment"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut found = None;\n    for item in items {\n        // Is this the one? Then return it below.\n        if item > 3 { found = Some(item); break; }\n    }\n}",
    "Set `found`";
    "comment with a question mark and return"
)]
#[test_case(
    "let mut found = None;\nfor entry in entries {\n    if entry.ready { found = Some(entry.id); break; }\n}",
    "Set `found` with an iterator search over `entries`";
    "edit snippet without enclosing function"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    'search: for x in xs { if x > 3 { found = Some(x); break; } }\n}",
    "Set `found`";
    "labeled loop with a plain break"
)]
#[test]
fn test_find_loop_interrupts(content: &str, expected_message: &str) {
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
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    for x in xs {\n        log(x);\n        if x > 3 { found = Some(x); break; }\n    }\n}";
    "statement before the if"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    for x in xs {\n        if x > 3 { found = Some(x); break; }\n        log(x);\n    }\n}";
    "statement after the if"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    for x in xs {\n        if x > 3 { log(x); found = Some(x); break; }\n    }\n}";
    "statement before the assignment"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    for x in xs {\n        if x > 3 { found = Some(x); log(x); break; }\n    }\n}";
    "statement between the assignment and the break"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut last = None;\n    for x in xs {\n        if x > 3 { last = Some(x); }\n    }\n}";
    "no break keeps the last match"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    for x in xs {\n        if x > 3 { found = Some(x); break; } else { skip(x); }\n    }\n}";
    "if with else"
)]
#[test_case(
    "fn f(xs: Vec<u8>, other: &mut Option<u8>) {\n    let mut found = None;\n    for x in xs {\n        if x > 3 { *other = Some(x); break; }\n    }\n}";
    "assigns a different place"
)]
#[test_case(
    "fn f(xs: Vec<u8>, mut other: Option<u8>) {\n    let mut found = None;\n    for x in xs {\n        if x > 3 { other = Some(x); break; }\n    }\n}";
    "assigns a variable other than the one declared before the loop"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    log();\n    for x in xs {\n        if x > 3 { found = Some(x); break; }\n    }\n}";
    "statement between the let and the loop"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut count = 0;\n    for x in xs {\n        if x > 3 { count = 1; break; }\n    }\n}";
    "initial value is not None or false"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    while let Some(x) = next() {\n        if x > 3 { found = Some(x); break; }\n    }\n}";
    "while loop"
)]
#[test_case(
    "fn f(rows: Vec<Vec<u8>>) {\n    let mut found = None;\n    'rows: for row in rows {\n        for x in row {\n            if x > 3 { found = Some(x); break 'rows; }\n        }\n    }\n}";
    "nested loops with a labeled break"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    for x in xs {\n        if x > 3 { found = Some(x); break 'outer; }\n    }\n}";
    "labeled break"
)]
#[test_case(
    "fn f(xs: Vec<u8>) -> Result<(), E> {\n    let mut found = None;\n    for x in xs {\n        if check(x)? { found = Some(x); break; }\n    }\n    Ok(())\n}";
    "question mark in the condition"
)]
#[test_case(
    "fn f(xs: Vec<u8>) -> Result<(), E> {\n    let mut found = None;\n    for x in xs {\n        if x > 3 { found = Some(parse(x)?); break; }\n    }\n    Ok(())\n}";
    "question mark in the assigned value"
)]
#[test_case(
    "async fn f(xs: Vec<u8>) {\n    let mut found = None;\n    for x in xs {\n        if check(x).await { found = Some(x); break; }\n    }\n}";
    "await in the condition"
)]
#[test_case(
    "fn f(xs: Vec<Option<u8>>) {\n    let mut found = None;\n    for x in xs {\n        if x.is_some() { found = Some(match x { Some(0) => return, y => y }); break; }\n    }\n}";
    "return inside the assigned value"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut found = None;\n    for x in xs {\n        #[cfg(unix)]\n        if x > 3 { found = Some(x); break; }\n    }\n}";
    "attribute on the if"
)]
#[test]
fn test_find_loop_passes(content: &str) {
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
