//! Rule evaluation for Claude Code hooks.
//!
//! Rules are simple functions that take a hook and return a response.
//! They are evaluated in sequence, and the first non-Passthrough response wins.

use regex::Regex;

use crate::claude::hook::{ContinueResponse, Hook, PreToolUsePayload, Response};

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

// =============================================================================
// Rules
// =============================================================================

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
        r#"Found indented 'use' statements in new content (lines: {lines})

This usually means you've placed import statements inside a function body.
Per the project style guide:

  "Never put import statements inside functions (unless the function is
   feature/cfg gated): always put them at file level"

Why this matters:
  - Rust convention: imports belong at the top of the file or module
  - Readability: dependencies should be visible at a glance
  - Consistency: all code in this project follows this pattern

What to do:
  - Move the 'use' statement to the top of the file with other imports
  - If this is legitimately needed (e.g., inside a #[cfg(test)] function),
    you can proceed anyway and I trust your judgment"#,
        lines = format_line_numbers(&lines)
    );

    Response::Continue(
        ContinueResponse::builder()
            .system_message(message)
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

    let pattern = Regex::new(r"^\s*let\s+(mut\s+)?[a-zA-Z_][a-zA-Z0-9_]*\s*:\s*").expect("valid regex");

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
        r#"Found left-hand side type annotations (lines: {lines})

Per the style guide, this pattern is discouraged. Prefer turbofish or inference.

Why this matters:
  - Type annotations on the left make code harder to scan
  - Turbofish syntax is more idiomatic and flexible in Rust
  - Type inference should be preferred when the type is obvious

What to do:
  - Use turbofish: `let foo = items.collect::<Vec<_>>()`
  - Use inference: `let foo = parse(input)` (compiler infers type)
  - Use helper methods: `let foo = items.collect_vec()` (with itertools)

The ONLY exceptions are function signatures and struct/enum definitions where
type annotations are syntactically required."#,
        lines = format_line_numbers(&lines)
    );

    Response::Continue(
        ContinueResponse::builder()
            .system_message(message)
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
        r#"Found potentially over-qualified paths (lines: {lines})

Per the style guide: "Prefer direct imports over fully qualified paths unless ambiguous"

Common mistakes:
  - color_eyre::eyre::eyre!("...") -> use eyre; eyre!("...")
  - client::courier::v1::Key::new() -> use client::courier::v1::Key; Key::new()
  - serde_json::json!({{...}}) -> This one is actually OK (keeps clarity)

When fully qualified paths ARE preferred:
  - When the name is ambiguous or unclear on its own
  - When multiple types with the same name exist
  - When it improves clarity (serde_json::to_string is clearer than to_string)

Review the flagged lines. If you can add a `use` statement at the top and
simplify the path, please do so. If the qualified path improves clarity or
avoids ambiguity, proceed as-is."#,
        lines = format_line_numbers(&lines)
    );

    Response::Continue(
        ContinueResponse::builder()
            .system_message(message)
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
        actions.push(
            "- Add import: `use pretty_assertions::assert_eq as pretty_assert_eq;`",
        );
        actions.push("- Use `pretty_assert_eq!` instead of `assert_eq!`");
    }

    let message = format!(
        r#"Found `assert_eq!` in test file without `pretty_assertions`

The `pretty_assertions` crate provides colorized diffs for assertion failures,
making it much easier to spot differences in complex structs or strings.

Why alias it?
  - `assert_eq` from std prelude conflicts with imported `assert_eq`
  - Using `pretty_assert_eq` avoids the conflict and makes it clear which you're using

What to do:
{actions}

Example:
```rust
use pretty_assertions::assert_eq as pretty_assert_eq;

#[test]
fn test_something() {{
    pretty_assert_eq!(actual, expected);
}}
```"#,
        actions = actions.join("\n")
    );

    Response::Continue(
        ContinueResponse::builder()
            .system_message(message)
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

    let field_pattern = Regex::new(r"^\s+\w+\s*:\s*\S").expect("valid regex");
    let variant_pattern = Regex::new(r"^\s+[A-Z]\w*\s*[,\(\{]").expect("valid regex");

    let lines: Vec<&str> = content.lines().collect();
    let mut violations = Vec::new();

    for i in 0..lines.len().saturating_sub(1) {
        let current = lines[i];
        let next = lines[i + 1];

        let current_trimmed = current.trim();
        let next_trimmed = next.trim();

        if current_trimmed.starts_with("//") || next_trimmed.starts_with("//") {
            continue;
        }
        if current_trimmed.is_empty() || next_trimmed.is_empty() {
            continue;
        }

        let current_is_field = field_pattern.is_match(current);
        let next_is_field = field_pattern.is_match(next);
        let current_is_variant = variant_pattern.is_match(current);
        let next_is_variant = variant_pattern.is_match(next);

        if (current_is_field && next_is_field) || (current_is_variant && next_is_variant) {
            violations.push(i + 1);
        }
    }

    if violations.is_empty() {
        return Response::Passthrough;
    }

    let message = format!(
        r#"Found consecutive struct fields or enum variants without blank lines (after lines: {lines})

Per the style guide, each field/variant should be separated by a blank line.

Why this matters:
  - Blank lines create visual separation for easier scanning
  - They provide natural space for documentation comments
  - Each field is conceptually distinct and deserves its own "paragraph"

What to do:
  - Add a blank line between each field/variant
  - Consider adding a doc comment (`///`) above each field explaining its purpose

Example:
```rust
struct Config {{
    /// The user's display name
    name: String,

    /// Maximum retry attempts before giving up
    max_retries: u32,
}}
```"#,
        lines = format_line_numbers(&violations)
    );

    Response::Continue(
        ContinueResponse::builder()
            .system_message(message)
            .build(),
    )
}
