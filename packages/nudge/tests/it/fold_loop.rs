//! Tests for the `prefer-fold-over-accumulator-loop` rule.
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

const RULE: &str = "prefer-fold-over-accumulator-loop";

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
    "fn f(items: &[String]) -> usize {\n    let mut total = 0;\n    for item in items {\n        total += item.len();\n    }\n    total\n}",
    "Compute `total` with an iterator over `items`";
    "sum with compound add"
)]
#[test_case(
    "fn f(xs: &[u64]) -> u64 {\n    let mut product: u64 = 1;\n    for x in xs.iter() { product *= x }\n    product\n}",
    "Compute `product` with an iterator over `xs.iter()`";
    "product without semicolon and with a type annotation"
)]
#[test_case(
    "fn f(xs: &[u8]) -> usize {\n    let mut n = 0;\n    for _x in xs {\n        n += 1;\n    }\n    n\n}",
    "`.count()` for `+= 1`";
    "count"
)]
#[test_case(
    "fn f(flags: &[u32]) -> u32 {\n    let mut mask = 0;\n    for flag in flags {\n        mask |= flag;\n    }\n    mask\n}",
    "Compute `mask`";
    "bitwise or compound assignment"
)]
#[test_case(
    "fn f(xs: &[u8]) -> u8 {\n    let mut best = 0;\n    for x in xs {\n        best = best.max(*x);\n    }\n    best\n}",
    "`.max()` or `.min()` for a running extreme";
    "method call on the accumulator"
)]
#[test_case(
    "fn f(parts: Vec<Part>) -> Acc {\n    let mut acc = Acc::default();\n    for part in parts {\n        acc = merge(acc, part);\n    }\n    acc\n}",
    "Compute `acc` with an iterator over `parts`";
    "function call with the accumulator as an argument"
)]
#[test_case(
    "fn f(xs: &[i32]) -> i32 {\n    let mut diff = 0;\n    for x in xs {\n        diff = diff - x;\n    }\n    diff\n}",
    "Compute `diff`";
    "binary expression with the accumulator on the left"
)]
#[test_case(
    "fn f(words: &[&str]) -> String {\n    let mut out = String::new();\n    // Join everything.\n    for word in words {\n        // Spaces come later.\n        out += word; // no separator\n        /* done */\n    }\n    out\n}",
    "`.collect::<String>()` for a string";
    "comments around the loop and the update"
)]
#[test_case(
    "fn f(xs: &[u8]) -> u32 {\n    let mut total = 0;\n    for x in xs {\n        // Is this right? Return early below if not.\n        total += u32::from(*x);\n    }\n    total\n}",
    "Compute `total`";
    "comment with a question mark and return"
)]
#[test_case(
    "let mut total = 0;\nfor entry in entries {\n    total += entry.size;\n}",
    "Compute `total` with an iterator over `entries`";
    "edit snippet without enclosing function"
)]
#[test_case(
    "fn f(xs: Vec<u8>) {\n    let mut total = 0;\n    'sum: for x in xs { total += x; }\n}",
    "Compute `total`";
    "labeled loop"
)]
#[test]
fn test_fold_loop_interrupts(content: &str, expected_message: &str) {
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
    "fn f(xs: &[u8]) {\n    let mut total = 0;\n    for x in xs {\n        log(x);\n        total += x;\n    }\n}";
    "statement before the update"
)]
#[test_case(
    "fn f(xs: &[u8]) {\n    let mut total = 0;\n    for x in xs {\n        total += x;\n        log(x);\n    }\n}";
    "statement after the update"
)]
#[test_case(
    "fn f(xs: &[u8]) {\n    let mut total = 0;\n    for x in xs {\n        total += x;\n        total += 1;\n    }\n}";
    "two updates"
)]
#[test_case(
    "fn f(xs: &[u8]) {\n    let mut total = 0;\n    for x in xs {\n        if *x > 3 { total += x; }\n    }\n}";
    "update inside an if"
)]
#[test_case(
    "fn f(xs: &[u8]) {\n    let mut total = 0;\n    log();\n    for x in xs {\n        total += x;\n    }\n}";
    "statement between the let and the loop"
)]
#[test_case(
    "fn f(xs: &[u8], other: &mut u32) {\n    let mut total = 0;\n    for x in xs {\n        *other += x;\n    }\n}";
    "updates a different place"
)]
#[test_case(
    "fn f(xs: &[u8], mut other: u32) {\n    let mut total = 0;\n    for x in xs {\n        other += x;\n    }\n}";
    "updates a variable other than the one declared before the loop"
)]
#[test_case(
    "fn f(xs: &[u8]) {\n    let mut last = 0;\n    for x in xs {\n        last = *x;\n    }\n}";
    "plain assignment that ignores the variable keeps the last item"
)]
#[test_case(
    "fn f(xs: &[u8]) {\n    let mut acc = 0;\n    for x in xs {\n        acc = combine(other, x);\n    }\n}";
    "call without the variable as an argument"
)]
#[test_case(
    "fn f(digits: &[u32]) {\n    let mut n = 0;\n    for d in digits {\n        n = n * 10 + d;\n    }\n}";
    "variable only nested deeper in the right side"
)]
#[test_case(
    "fn f(n: usize) {\n    let mut total = 0;\n    for _ in 0..n {\n        total += 2;\n    }\n}";
    "repeat count with an underscore pattern"
)]
#[test_case(
    "fn f() {\n    let mut total = 0;\n    while let Some(x) = next() {\n        total += x;\n    }\n}";
    "while loop"
)]
#[test_case(
    "fn f(xs: &[&str]) -> Result<u32, E> {\n    let mut total = 0;\n    for x in xs {\n        total += x.parse::<u32>()?;\n    }\n    Ok(total)\n}";
    "question mark in the update"
)]
#[test_case(
    "async fn f(xs: Vec<Job>) {\n    let mut total = 0;\n    for x in xs {\n        total += x.run().await;\n    }\n}";
    "await in the update"
)]
#[test_case(
    "fn f(xs: &[Option<u8>]) {\n    let mut total = 0;\n    for x in xs {\n        total += match x { Some(v) => *v, None => continue };\n    }\n}";
    "continue inside the update"
)]
#[test_case(
    "fn f(xs: &[u8]) {\n    let mut total = 0;\n    for x in xs {\n        #[cfg(unix)]\n        total += x;\n    }\n}";
    "attribute on the update"
)]
#[test_case(
    "fn f(items: &[Item]) -> Result<usize, E> {\n    let mut bytes = 0;\n    for item in items {\n        bytes += try_write!(item.write());\n    }\n    Ok(bytes)\n}";
    "macro that may return early in the update"
)]
#[test_case(
    "fn f(xs: &mut [u32]) -> u32 {\n    let mut carry = 0;\n    for x in xs.iter_mut() {\n        carry = mul(x, carry);\n    }\n    carry\n}";
    "iter_mut loop that writes elements"
)]
#[test_case(
    "fn f(xs: &mut Vec<u32>) -> u32 {\n    let mut carry = 0;\n    for x in &mut *xs {\n        carry = mul(x, carry);\n    }\n    carry\n}";
    "mutable borrow loop that writes elements"
)]
#[test]
fn test_fold_loop_passes(content: &str) {
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
