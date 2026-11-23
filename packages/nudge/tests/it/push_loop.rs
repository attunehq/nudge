//! Tests for the `prefer-collect-over-push-loop` rule.
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

const RULE: &str = "prefer-collect-over-push-loop";

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
    "fn f(items: Vec<u8>) -> Vec<u8> {\n    let mut doubled = Vec::new();\n    for item in items {\n        doubled.push(item * 2);\n    }\n    doubled\n}",
    "Build `doubled` by collecting an iterator over `items`";
    "plain push"
)]
#[test_case(
    "fn f(items: &[u8]) {\n    let mut out = vec![];\n    for x in items.iter() { out.push(x + 1) }\n}",
    "Build `out` by collecting an iterator over `items.iter()`";
    "vec macro and push without semicolon"
)]
#[test_case(
    "fn f(items: &[u8]) {\n    let mut out = Vec::with_capacity(items.len());\n    for x in items {\n        out.push(*x);\n    }\n}",
    "Build `out` by collecting an iterator over `items`";
    "with_capacity"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut results = Vec::new();\n    for item in items {\n        if let Some(value) = process(item) {\n            results.push(value);\n        }\n    }\n}",
    "Build `results`";
    "if let push"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut evens = Vec::new();\n    for item in items {\n        if item % 2 == 0 { evens.push(item); }\n    }\n}",
    "Build `evens`";
    "if push"
)]
#[test_case(
    "fn f(lines: &str) -> Result<Vec<Entry>, String> {\n    let mut entries = Vec::new();\n    for line in lines.lines() {\n        entries.push(line.parse::<Entry>()?);\n    }\n    Ok(entries)\n}",
    "`.collect::<Result<Vec<_>, _>>()?` (`Option<Vec<_>>` for an `Option`)";
    "push with question mark"
)]
#[test_case(
    "fn f(items: &[Item]) {\n    let mut kept = Vec::new();\n    for item in items {\n        if item.ok { kept.push(item.id); }\n    }\n}",
    "Build `kept`";
    "if condition on a field"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut kept = Vec::new();\n    // Keep the positive ones.\n    for item in items {\n        // Only positives.\n        if item > 0 {\n            kept.push(item); // keep\n        }\n        /* done */\n    }\n}",
    "Build `kept`";
    "comments around the loop and the push"
)]
#[test_case(
    "let mut names = Vec::new();\nfor entry in entries {\n    names.push(entry.name.clone());\n}",
    "Build `names` by collecting an iterator over `entries`";
    "edit snippet without enclosing function"
)]
#[test_case(
    "        let mut names: Vec<String> = Vec::new();\n        for entry in entries {\n            names.push(entry.name.clone());\n        }",
    "Build `names`";
    "indented edit snippet with a type annotation"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut v = Vec::new();\n    'outer: for x in xs { v.push(x); }\n}",
    "Build `v`";
    "labeled loop"
)]
#[test]
fn test_push_loop_interrupts(content: &str, expected_message: &str) {
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
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        v.push(x);\n        log(x);\n    }\n}";
    "statement after the push"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        log(x);\n        v.push(x);\n    }\n}";
    "statement before the push"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        let y = x + 1;\n        v.push(y);\n    }\n}";
    "let before the push"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        if x > 0 { log(x); v.push(x); }\n    }\n}";
    "statement before the push inside if"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        if x > 0 { v.push(x); } else { w.push(x); }\n    }\n}";
    "if with else"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        if x > 0 { v.push(x); v.push(x); }\n    }\n}";
    "two pushes"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        if x > 0 { break; }\n        v.push(x);\n    }\n}";
    "early break"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        other.push(x);\n    }\n}";
    "push to a different collection"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = vec![1];\n    for x in items { v.push(x); }\n}";
    "non-empty vec macro"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    let y = 1;\n    for x in items { v.push(x + y); }\n}";
    "statement between the let and the loop"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let v = Vec::new();\n    for x in items { v.push(x); }\n}";
    "immutable binding"
)]
#[test_case(
    "fn f(items: Vec<char>) {\n    let mut s = String::new();\n    for c in items { s.push(c); }\n}";
    "string push"
)]
#[test_case(
    "fn f(items: Vec<Vec<u8>>) {\n    let mut v = Vec::new();\n    for x in items { v.extend(x); }\n}";
    "extend instead of push"
)]
#[test_case(
    "async fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items { v.push(fetch(x).await); }\n}";
    "await in the push"
)]
#[test_case(
    "fn f(items: Vec<u8>) {\n    let mut v = Vec::new();\n    for x in items {\n        #[cfg(unix)]\n        v.push(x);\n    }\n}";
    "attribute on the push"
)]
#[test_case(
    "fn f(mut queue: Vec<u8>) {\n    let mut v = Vec::new();\n    while let Some(x) = queue.pop() { v.push(x); }\n}";
    "while loop"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut seen = Vec::new();\n    for x in xs {\n        if !seen.contains(&x) { seen.push(x); }\n    }\n}";
    "order-preserving dedup reads the vec"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut out = Vec::new();\n    for x in xs {\n        if out.last() != Some(&x) { out.push(x); }\n    }\n}";
    "consecutive dedup reads the vec"
)]
#[test_case(
    "fn f(xs: Vec<u32>) {\n    let mut sums = Vec::new();\n    for x in xs {\n        sums.push(sums.last().copied().unwrap_or(0) + x);\n    }\n}";
    "running total reads the vec"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut v = Vec::new();\n    for _ in xs { v.push(v.len() * 2); }\n}";
    "push reads the vec length"
)]
#[test_case(
    "fn f(xs: Vec<Option<u8>>) {\n    let mut v = Vec::new();\n    for x in xs {\n        v.push(match x { Some(y) => y, None => break });\n    }\n}";
    "break inside the pushed value"
)]
#[test_case(
    "fn f(xs: Vec<u8>) -> Result<Vec<u8>, E> {\n    let mut v = Vec::new();\n    for x in xs {\n        if check(x)? { v.push(x); }\n    }\n    Ok(v)\n}";
    "question mark in the if condition"
)]
#[test_case(
    "fn f(xs: Vec<Option<u8>>) {\n    let mut v = Vec::new();\n    for x in xs {\n        if let Some(y) = x { v.push(match y { 0 => continue, n => n }); }\n    }\n}";
    "continue inside the pushed value"
)]
#[test]
fn test_push_loop_passes(content: &str) {
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
