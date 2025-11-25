//! Integration tests for the rules evaluation system.

use pretty_assertions::assert_eq as pretty_assert_eq;
use simple_test_case::test_case;
use xshell::{Shell, cmd};

/// Expected outcome from running a hook through pavlov.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum Expected {
    /// Passthrough: exit 0, no output
    Passthrough,
    /// Continue: exit 0, output contains "continue":true
    Continue,
    /// Interrupt: exit 2, output contains "continue":false
    Interrupt,
}

/// Build a PreToolUse hook JSON payload for Write tool.
fn write_hook(file_path: &str, content: &str) -> String {
    use serde_json::json;
    json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "tool_name": "Write",
        "tool_use_id": "123",
        "tool_input": {
            "file_path": file_path,
            "content": content
        }
    })
    .to_string()
}

/// Build a PreToolUse hook JSON payload for Edit tool.
fn edit_hook(file_path: &str, new_string: &str) -> String {
    serde_json::json!({
        "hook_event_name": "PreToolUse",
        "session_id": "test",
        "transcript_path": "/tmp/test",
        "permission_mode": "default",
        "cwd": "/tmp",
        "tool_name": "Edit",
        "tool_use_id": "123",
        "tool_input": {
            "file_path": file_path,
            "old_string": "placeholder",
            "new_string": new_string
        }
    })
    .to_string()
}

/// Run pavlov claude hook with the given input JSON and return (exit_code, output).
/// Output combines stdout and stderr since Interrupt writes to stderr, Continue to stdout.
fn run_hook(sh: &Shell, input: &str) -> (i32, String) {
    let output = cmd!(sh, "cargo run --quiet -p pavlov -- claude hook")
        .stdin(input)
        .ignore_status()
        .output()
        .expect("failed to run pavlov");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    (exit_code, combined)
}

// =============================================================================
// Inline Imports Rule Tests
// =============================================================================

#[test_case(
    "fn main() {\n    use std::io;\n}",
    Expected::Interrupt;
    "indented use statement triggers guidance"
)]
#[test_case(
    "use std::io;\n\nfn main() {}",
    Expected::Passthrough;
    "top-level use statement passes"
)]
#[test_case(
    "fn main() {\n    // use std::io;\n}",
    Expected::Passthrough;
    "commented use statement passes"
)]
#[test]
fn test_inline_imports(content: &str, expected: Expected) {
    let sh = Shell::new().unwrap();
    let input = write_hook("test.rs", content);
    let (exit_code, stdout) = run_hook(&sh, &input);

    match expected {
        Expected::Passthrough => {
            pretty_assert_eq!(exit_code, 0, "expected passthrough (exit 0)");
            assert!(stdout.is_empty(), "expected no output for passthrough");
        }
        Expected::Continue => {
            pretty_assert_eq!(exit_code, 0, "expected continue (exit 0)");
            assert!(stdout.contains(r#""continue":true"#), "expected continue:true in output");
        }
        Expected::Interrupt => {
            pretty_assert_eq!(exit_code, 2, "expected interrupt (exit 2)");
            assert!(stdout.contains(r#""continue":false"#), "expected continue:false in output");
        }
    }
}

// =============================================================================
// LHS Type Annotations Rule Tests
// =============================================================================

#[test_case(
    "fn main() {\n    let foo: Vec<String> = vec![];\n}",
    Expected::Interrupt;
    "lhs type annotation triggers guidance"
)]
#[test_case(
    "fn main() {\n    let foo = vec![\"hello\".to_string()];\n}",
    Expected::Passthrough;
    "inference passes"
)]
#[test_case(
    "fn main() {\n    let foo = items.collect::<Vec<_>>();\n}",
    Expected::Passthrough;
    "turbofish passes"
)]
#[test_case(
    "// let foo: Type = bar;\nfn main() {}",
    Expected::Passthrough;
    "commented let passes"
)]
#[test]
fn test_lhs_type_annotations(content: &str, expected: Expected) {
    let sh = Shell::new().unwrap();
    let input = write_hook("test.rs", content);
    let (exit_code, stdout) = run_hook(&sh, &input);

    match expected {
        Expected::Passthrough => {
            pretty_assert_eq!(exit_code, 0, "expected passthrough (exit 0)");
            assert!(stdout.is_empty(), "expected no output for passthrough");
        }
        Expected::Continue => {
            pretty_assert_eq!(exit_code, 0, "expected continue (exit 0)");
            assert!(stdout.contains(r#""continue":true"#), "expected continue:true in output");
        }
        Expected::Interrupt => {
            pretty_assert_eq!(exit_code, 2, "expected interrupt (exit 2)");
            assert!(stdout.contains(r#""continue":false"#), "expected continue:false in output");
        }
    }
}

// =============================================================================
// Qualified Paths Rule Tests
// =============================================================================

#[test_case(
    "fn main() {\n    color_eyre::eyre::eyre!(\"error\");\n}",
    Expected::Interrupt;
    "over-qualified path triggers guidance"
)]
#[test_case(
    "use color_eyre::eyre::eyre;\n\nfn main() {\n    eyre!(\"error\");\n}",
    Expected::Passthrough;
    "imported and used directly passes"
)]
#[test_case(
    "// color_eyre::eyre::eyre!()\nfn main() {}",
    Expected::Passthrough;
    "commented qualified path passes"
)]
#[test_case(
    "use foo::bar::baz;\n\nfn main() {}",
    Expected::Passthrough;
    "qualified path in use statement passes"
)]
#[test]
fn test_qualified_paths(content: &str, expected: Expected) {
    let sh = Shell::new().unwrap();
    let input = write_hook("test.rs", content);
    let (exit_code, stdout) = run_hook(&sh, &input);

    match expected {
        Expected::Passthrough => {
            pretty_assert_eq!(exit_code, 0, "expected passthrough (exit 0)");
            assert!(stdout.is_empty(), "expected no output for passthrough");
        }
        Expected::Continue => {
            pretty_assert_eq!(exit_code, 0, "expected continue (exit 0)");
            assert!(stdout.contains(r#""continue":true"#), "expected continue:true in output");
        }
        Expected::Interrupt => {
            pretty_assert_eq!(exit_code, 2, "expected interrupt (exit 2)");
            assert!(stdout.contains(r#""continue":false"#), "expected continue:false in output");
        }
    }
}

// =============================================================================
// Non-Rust File Tests
// =============================================================================

#[test_case("test.py", "    use std::io;"; "python file passes")]
#[test_case("test.js", "    use std::io;"; "javascript file passes")]
#[test_case("test.txt", "let foo: Type = bar;"; "text file passes")]
#[test]
fn test_non_rust_files_pass(file_path: &str, content: &str) {
    let sh = Shell::new().unwrap();
    let input = write_hook(file_path, content);
    let (exit_code, stdout) = run_hook(&sh, &input);

    pretty_assert_eq!(exit_code, 0, "non-rust files should passthrough");
    assert!(stdout.is_empty(), "non-rust files should have no output");
}

// =============================================================================
// Edit Tool Tests
// =============================================================================

#[test]
fn test_edit_tool_triggers_rules() {
    let sh = Shell::new().unwrap();
    let input = edit_hook("test.rs", "    use std::io;\n");
    let (exit_code, stdout) = run_hook(&sh, &input);

    pretty_assert_eq!(exit_code, 2, "edit tool should trigger inline imports rule");
    assert!(stdout.contains(r#""continue":false"#), "expected interrupt response");
}

// =============================================================================
// Rule Priority Tests
// =============================================================================

#[test]
fn test_first_matching_rule_wins() {
    let sh = Shell::new().unwrap();
    // Content triggers both inline imports and LHS annotations rules
    let content = "fn main() {\n    use std::io;\n    let foo: Type = bar;\n}";
    let input = write_hook("test.rs", content);
    let (exit_code, stdout) = run_hook(&sh, &input);

    // Inline imports rule is first in the list, so it should be the one that fires
    pretty_assert_eq!(exit_code, 2, "expected interrupt");
    assert!(stdout.contains("BLOCKED"), "inline imports rule should fire first");
}

// =============================================================================
// Pretty Assertions Rule Tests
// =============================================================================

#[test_case(
    "tests/foo.rs",
    "#[test]\nfn test_it() {\n    assert_eq!(1, 1);\n}",
    Expected::Interrupt;
    "test file with assert_eq triggers guidance"
)]
#[test_case(
    "tests/foo.rs",
    "use pretty_assertions::assert_eq as pretty_assert_eq;\n\n#[test]\nfn test_it() {\n    pretty_assert_eq!(1, 1);\n}",
    Expected::Passthrough;
    "test file with aliased import passes"
)]
#[test_case(
    "tests/foo.rs",
    "use pretty_assertions::assert_eq;\n\n#[test]\nfn test_it() {\n    assert_eq!(1, 1);\n}",
    Expected::Interrupt;
    "test file with unaliased import triggers guidance"
)]
#[test_case(
    "src/lib.rs",
    "fn main() {\n    assert_eq!(1, 1);\n}",
    Expected::Passthrough;
    "non-test file with assert_eq passes"
)]
#[test_case(
    "tests/foo.rs",
    "#[test]\nfn test_it() {\n    assert!(true);\n}",
    Expected::Passthrough;
    "test file without assert_eq passes"
)]
#[test_case(
    "src/lib.rs",
    "#[test]\nfn test_it() {\n    assert_eq!(1, 1);\n}",
    Expected::Interrupt;
    "file with test attribute triggers guidance"
)]
#[test]
fn test_pretty_assertions(file_path: &str, content: &str, expected: Expected) {
    let sh = Shell::new().unwrap();
    let input = write_hook(file_path, content);
    let (exit_code, stdout) = run_hook(&sh, &input);

    match expected {
        Expected::Passthrough => {
            pretty_assert_eq!(exit_code, 0, "expected passthrough (exit 0)");
            assert!(stdout.is_empty(), "expected no output for passthrough");
        }
        Expected::Continue => {
            pretty_assert_eq!(exit_code, 0, "expected continue (exit 0)");
            assert!(stdout.contains(r#""continue":true"#), "expected continue:true in output");
        }
        Expected::Interrupt => {
            pretty_assert_eq!(exit_code, 2, "expected interrupt (exit 2)");
            assert!(stdout.contains(r#""continue":false"#), "expected continue:false in output");
        }
    }
}

#[test]
fn test_pretty_assertions_unaliased_import_message() {
    let sh = Shell::new().unwrap();
    let content = "use pretty_assertions::assert_eq;\n\n#[test]\nfn test_it() {\n    assert_eq!(1, 1);\n}";
    let input = write_hook("tests/foo.rs", content);
    let (_, stdout) = run_hook(&sh, &input);

    assert!(
        stdout.contains("Change the import"),
        "should suggest changing the import"
    );
    assert!(
        stdout.contains("pretty_assert_eq"),
        "should mention the alias"
    );
}

// =============================================================================
// Field Spacing Rule Tests
// =============================================================================

#[test_case(
    "struct Foo {\n    a: String,\n    b: String,\n}",
    Expected::Interrupt;
    "consecutive struct fields triggers guidance"
)]
#[test_case(
    "struct Foo {\n    a: String,\n\n    b: String,\n}",
    Expected::Passthrough;
    "spaced struct fields passes"
)]
#[test_case(
    "enum Foo {\n    A,\n    B,\n}",
    Expected::Interrupt;
    "consecutive enum variants triggers guidance"
)]
#[test_case(
    "enum Foo {\n    A,\n\n    B,\n}",
    Expected::Passthrough;
    "spaced enum variants passes"
)]
#[test_case(
    "struct Foo {\n    a: String,\n}",
    Expected::Passthrough;
    "single field struct passes"
)]
#[test_case(
    "struct Foo {\n    a: String,\n    /// Doc comment\n    b: String,\n}",
    Expected::Interrupt;
    "fields separated by comment without blank line triggers guidance"
)]
#[test_case(
    "struct Foo {\n    a: String,\n\n    /// Doc comment\n    b: String,\n}",
    Expected::Passthrough;
    "fields with blank line before doc comment passes"
)]
#[test_case(
    "enum Foo {\n    A(String),\n    B { x: i32 },\n}",
    Expected::Interrupt;
    "tuple and struct variants without spacing triggers guidance"
)]
#[test]
fn test_field_spacing(content: &str, expected: Expected) {
    let sh = Shell::new().unwrap();
    let input = write_hook("src/types.rs", content);
    let (exit_code, stdout) = run_hook(&sh, &input);

    match expected {
        Expected::Passthrough => {
            pretty_assert_eq!(exit_code, 0, "expected passthrough (exit 0)");
            assert!(stdout.is_empty(), "expected no output for passthrough");
        }
        Expected::Continue => {
            pretty_assert_eq!(exit_code, 0, "expected continue (exit 0)");
            assert!(stdout.contains(r#""continue":true"#), "expected continue:true in output");
        }
        Expected::Interrupt => {
            pretty_assert_eq!(exit_code, 2, "expected interrupt (exit 2)");
            assert!(stdout.contains(r#""continue":false"#), "expected continue:false in output");
        }
    }
}
