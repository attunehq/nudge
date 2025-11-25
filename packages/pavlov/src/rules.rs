//! Rule evaluation for Claude Code hooks.
//!
//! Rules are simple functions that take a hook and return a response.
//! They are evaluated in sequence, and the first non-Passthrough response wins.

use regex::Regex;

use crate::claude::hook::{Hook, InterruptResponse, PreToolUseOutput, PreToolUsePayload, Response};

/// Evaluate all rules against a hook. First non-Passthrough response wins.
pub fn evaluate_all(hook: &Hook) -> Response {
    let rules: &[fn(&Hook) -> Response] = &[
        no_inline_imports,
        no_lhs_type_annotations,
        no_qualified_paths,
        prefer_pretty_assertions,
        require_field_spacing,
    ];

    for rule in rules {
        let response = rule(hook);
        if !matches!(response, Response::Passthrough) {
            return response;
        }
    }

    Response::Passthrough
}

/// Extract file_path and content from Write/Edit tool inputs.
fn extract_file_content(payload: &PreToolUsePayload) -> Option<(&str, &str)> {
    match payload.tool_name.as_str() {
        "Write" => Some((
            payload.tool_input.get("file_path")?.as_str()?,
            payload.tool_input.get("content")?.as_str()?,
        )),
        "Edit" => Some((
            payload.tool_input.get("file_path")?.as_str()?,
            payload.tool_input.get("new_string")?.as_str()?,
        )),
        _ => None,
    }
}

/// Check if a file path has a Rust extension.
fn is_rust_file(path: &str) -> bool {
    path.ends_with(".rs")
}

/// Find line numbers where a pattern matches.
fn find_matching_lines(content: &str, pattern: &Regex) -> Vec<usize> {
    content
        .lines()
        .enumerate()
        .filter(|(_, line)| pattern.is_match(line))
        .map(|(i, _)| i + 1) // 1-indexed
        .collect()
}

/// Format line numbers as a comma-separated string.
fn format_line_numbers(lines: &[usize]) -> String {
    lines
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Catch `use` statements inside function bodies.
///
/// Pattern: lines starting with whitespace followed by `use `
fn no_inline_imports(hook: &Hook) -> Response {
    let Hook::PreToolUse(payload) = hook else {
        return Response::Passthrough;
    };

    let Some((file_path, content)) = extract_file_content(payload) else {
        return Response::Passthrough;
    };

    if !is_rust_file(file_path) {
        return Response::Passthrough;
    }

    let pattern = Regex::new(r"^\s+use ").expect("valid regex");
    let lines = find_matching_lines(content, &pattern);

    if lines.is_empty() {
        return Response::Passthrough;
    }

    let message = format!(
        "Move the `use` statement(s) on lines {} to the top of the file with other imports, then retry this exact operation.",
        format_line_numbers(&lines)
    );

    Response::Interrupt(
        InterruptResponse::builder()
            .stop_reason("BLOCKED: Import statements inside function body. Fix and retry immediately.")
            .system_message(message)
            .hook_specific_output(serde_json::to_value(PreToolUseOutput::default()).unwrap())
            .build(),
    )
}

/// Catch left-hand side type annotations in variable declarations.
///
/// Pattern: `let name: Type = ...` or `let mut name: Type = ...`
/// Excludes: comments
fn no_lhs_type_annotations(hook: &Hook) -> Response {
    let Hook::PreToolUse(payload) = hook else {
        return Response::Passthrough;
    };

    let Some((file_path, content)) = extract_file_content(payload) else {
        return Response::Passthrough;
    };

    if !is_rust_file(file_path) {
        return Response::Passthrough;
    }

    let pattern =
        Regex::new(r"^\s*let\s+(mut\s+)?[a-zA-Z_][a-zA-Z0-9_]*\s*:\s*").expect("valid regex");

    let lines: Vec<usize> = content
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim();
            // Exclude comments
            !trimmed.starts_with("//") && pattern.is_match(line)
        })
        .map(|(i, _)| i + 1)
        .collect();

    if lines.is_empty() {
        return Response::Passthrough;
    }

    let message = format!(
        "Remove LHS type annotations on lines {}. Use turbofish (`collect::<Vec<_>>()`) or type inference instead, then retry.",
        format_line_numbers(&lines)
    );

    Response::Interrupt(
        InterruptResponse::builder()
            .stop_reason("BLOCKED: Left-hand side type annotations. Use turbofish or inference instead.")
            .system_message(message)
            .hook_specific_output(serde_json::to_value(PreToolUseOutput::default()).unwrap())
            .build(),
    )
}

/// Catch unnecessarily fully qualified paths.
///
/// Pattern: paths with 2+ `::` separators (e.g., `foo::bar::baz`)
/// Excludes: `use` statements, `mod` declarations, comments
fn no_qualified_paths(hook: &Hook) -> Response {
    let Hook::PreToolUse(payload) = hook else {
        return Response::Passthrough;
    };

    let Some((file_path, content)) = extract_file_content(payload) else {
        return Response::Passthrough;
    };

    if !is_rust_file(file_path) {
        return Response::Passthrough;
    }

    let pattern =
        Regex::new(r"[a-zA-Z_][a-zA-Z0-9_]*(::[a-zA-Z_][a-zA-Z0-9_]*){2,}").expect("valid regex");

    let lines: Vec<usize> = content
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim();
            // Exclude use statements, mod declarations, and comments
            !trimmed.starts_with("use ")
                && !trimmed.starts_with("mod ")
                && !trimmed.starts_with("//")
                && pattern.is_match(line)
        })
        .map(|(i, _)| i + 1)
        .collect();

    if lines.is_empty() {
        return Response::Passthrough;
    }

    let message = format!(
        "Simplify qualified paths on lines {} by adding `use` imports at file top, then retry. Exception: keep qualified if it improves clarity (e.g., `serde_json::to_string`).",
        format_line_numbers(&lines)
    );

    Response::Interrupt(
        InterruptResponse::builder()
            .stop_reason("BLOCKED: Over-qualified paths. Add imports and simplify.")
            .system_message(message)
            .hook_specific_output(serde_json::to_value(PreToolUseOutput::default()).unwrap())
            .build(),
    )
}

/// Check if a file is a test file.
fn is_test_file(path: &str, content: &str) -> bool {
    path.contains("/tests/") || path.ends_with("_test.rs") || content.contains("#[test]")
}

/// Suggest using `pretty_assertions` for better test output.
///
/// Triggers when:
///   - File is a test file (tests/*.rs, *_test.rs, or contains #[test])
///   - Content uses `assert_eq!`
///
/// Provides contextual guidance based on what's already in the file.
fn prefer_pretty_assertions(hook: &Hook) -> Response {
    let Hook::PreToolUse(payload) = hook else {
        return Response::Passthrough;
    };

    let Some((file_path, content)) = extract_file_content(payload) else {
        return Response::Passthrough;
    };

    if !is_rust_file(file_path) {
        return Response::Passthrough;
    }

    if !is_test_file(file_path, content) {
        return Response::Passthrough;
    }

    let has_assert_eq_usage = content.contains("assert_eq!");
    if !has_assert_eq_usage {
        return Response::Passthrough;
    }

    let has_aliased_import = content.contains("pretty_assertions::assert_eq as pretty_assert_eq");
    if has_aliased_import {
        return Response::Passthrough;
    }

    let has_unaliased_import = content.contains("use pretty_assertions::assert_eq;");

    let mut actions = Vec::new();

    if has_unaliased_import {
        actions.push(
            "- Change the import to: `use pretty_assertions::assert_eq as pretty_assert_eq;`",
        );
        actions.push("- Replace `assert_eq!` with `pretty_assert_eq!` in your tests");
    } else {
        actions.push("- Ensure `pretty_assertions` is in dev-dependencies (run `cargo add pretty_assertions --dev` if needed)");
        actions.push("- Add import: `use pretty_assertions::assert_eq as pretty_assert_eq;`");
        actions.push("- Use `pretty_assert_eq!` instead of `assert_eq!`");
    }

    let message = format!(
        "Add `use pretty_assertions::assert_eq as pretty_assert_eq;` and use `pretty_assert_eq!` instead of `assert_eq!`, then retry.\n\nRequired changes:\n{}",
        actions.join("\n")
    );

    Response::Interrupt(
        InterruptResponse::builder()
            .stop_reason("BLOCKED: Use pretty_assertions in tests for better diff output.")
            .system_message(message)
            .hook_specific_output(serde_json::to_value(PreToolUseOutput::default()).unwrap())
            .build(),
    )
}

/// Require blank lines between struct fields and enum variants.
///
/// Catches consecutive field/variant definitions without spacing.
fn require_field_spacing(hook: &Hook) -> Response {
    let Hook::PreToolUse(payload) = hook else {
        return Response::Passthrough;
    };

    let Some((file_path, content)) = extract_file_content(payload) else {
        return Response::Passthrough;
    };

    if !is_rust_file(file_path) {
        return Response::Passthrough;
    }

    let field_pattern =
        Regex::new(r"^\s+(pub(\s*\([^)]*\))?\s+)?\w+\s*:\s*\S").expect("valid regex");
    let variant_pattern = Regex::new(r"^\s+[A-Z]\w*\s*[,\(\{]").expect("valid regex");

    let lines: Vec<&str> = content.lines().collect();
    let mut violations = Vec::new();

    for i in 0..lines.len().saturating_sub(1) {
        let current = lines[i];
        let next = lines[i + 1];

        let current_trimmed = current.trim();
        let next_trimmed = next.trim();

        // Skip if current line is a comment (we only care about field/variant -> something)
        if current_trimmed.starts_with("//") {
            continue;
        }
        if current_trimmed.is_empty() || next_trimmed.is_empty() {
            continue;
        }

        let current_is_field = field_pattern.is_match(current);
        let current_is_variant = variant_pattern.is_match(current);

        // A field/variant followed by a doc comment means next field's docs started without blank line
        let next_is_doc_comment = next_trimmed.starts_with("///");
        let next_is_field = field_pattern.is_match(next);
        let next_is_variant = variant_pattern.is_match(next);

        if current_is_field && (next_is_field || next_is_doc_comment) {
            violations.push(i + 1);
        }
        if current_is_variant && (next_is_variant || next_is_doc_comment) {
            violations.push(i + 1);
        }
    }

    if violations.is_empty() {
        return Response::Passthrough;
    }

    let message = format!(
        "Add a blank line after lines {}, then retry.",
        format_line_numbers(&violations)
    );

    Response::Interrupt(
        InterruptResponse::builder()
            .stop_reason("BLOCKED: Missing blank lines between struct fields/enum variants.")
            .system_message(message)
            .hook_specific_output(serde_json::to_value(PreToolUseOutput::default()).unwrap())
            .build(),
    )
}
