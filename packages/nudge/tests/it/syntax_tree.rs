//! Integration tests for SyntaxTree matcher.
//!
//! These tests verify that tree-sitter based matching works through the full
//! hook pipeline, including correct handling of AST-based patterns that regex
//! cannot express.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use pretty_assertions::assert_eq as pretty_assert_eq;
use tempfile::TempDir;

/// Create a temporary directory with a .nudge.yaml config containing the given rules.
fn setup_config(rules_yaml: &str) -> TempDir {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join(".nudge.yaml");
    std::fs::write(&config_path, rules_yaml).expect("write config");
    dir
}

/// Get the path to the built nudge binary.
fn get_binary_path() -> PathBuf {
    // Build the binary
    let status = Command::new("cargo")
        .args(["build", "--quiet", "-p", "nudge"])
        .status()
        .expect("failed to build nudge");
    assert!(status.success(), "cargo build failed");

    // Get the target directory - use CARGO_TARGET_DIR or default
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    workspace_root.join("target/debug/nudge")
}

/// Run nudge claude hook with the given input JSON in the specified directory.
fn run_hook_in_dir(dir: &TempDir, input: &str) -> (i32, String) {
    let binary = get_binary_path();

    // Run the binary directly with the temp dir as cwd
    let mut child = Command::new(&binary)
        .args(["claude", "hook"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn nudge");

    {
        let stdin = child.stdin.as_mut().expect("failed to get stdin");
        stdin
            .write_all(input.as_bytes())
            .expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait for nudge");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let combined = if !stdout.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };

    (exit_code, combined)
}

/// Build a PreToolUse hook JSON payload for Write tool.
fn write_hook(file_path: &str, content: &str) -> String {
    serde_json::json!({
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

#[test]
fn test_syntax_tree_matches_use_in_function_body() {
    let config = r#"
version: 1
rules:
  - name: no-inline-imports-ast
    description: Move imports to the top of the file (AST-based)
    message: "Move this `use` to the top of the file, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: "(function_item body: (block (use_declaration) @use))"
"#;

    let dir = setup_config(config);

    // This should trigger: use inside function body
    let input = write_hook("test.rs", "fn main() {\n    use std::io;\n}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for use inside function, got: {output}"
    );
}

#[test]
fn test_syntax_tree_passes_top_level_use() {
    let config = r#"
version: 1
rules:
  - name: no-inline-imports-ast
    description: Move imports to the top of the file (AST-based)
    message: "Move this `use` to the top of the file, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: "(function_item body: (block (use_declaration) @use))"
"#;

    let dir = setup_config(config);

    // This should pass: use at top level, not inside function
    let input = write_hook("test.rs", "use std::io;\n\nfn main() {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for top-level use, got: {output}"
    );
}

#[test]
fn test_syntax_tree_passes_use_in_mod_test() {
    // This is the key advantage over regex: we can distinguish use in function
    // bodies from use in mod test blocks.
    let config = r#"
version: 1
rules:
  - name: no-inline-imports-ast
    description: Move imports to the top of the file (AST-based)
    message: "Move this `use` to the top of the file, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: "(function_item body: (block (use_declaration) @use))"
"#;

    let dir = setup_config(config);

    // This should pass: use inside mod test (not a function body)
    let input = write_hook(
        "test.rs",
        r#"
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {}
}
"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for use in mod test, got: {output}"
    );
}

#[test]
fn test_syntax_tree_with_suggestion_interpolation() {
    let config = r#"
version: 1
rules:
  - name: function-name-check
    description: Check function naming
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: "(function_item name: (identifier) @fn_name)"
            suggestion: "Found function named `{{ $fn_name }}`"
"#;

    let dir = setup_config(config);

    let input = write_hook("test.rs", "fn my_test_function() {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains("my_test_function"),
        "expected suggestion to contain function name, got: {output}"
    );
}

// TypeScript Tests

#[test]
fn test_typescript_syntax_tree_matches_console_log() {
    let config = r#"
version: 1
rules:
  - name: no-console-log
    description: Remove console.log statements
    message: "Remove console.log before committing."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (call_expression
                function: (member_expression
                  object: (identifier) @obj
                  property: (property_identifier) @prop)
                (#eq? @obj "console")
                (#eq? @prop "log"))
"#;

    let dir = setup_config(config);

    // This should trigger: console.log in TypeScript file
    let input = write_hook("test.ts", "function main() {\n    console.log('hello');\n}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for console.log, got: {output}"
    );
}

#[test]
fn test_typescript_syntax_tree_passes_without_console_log() {
    let config = r#"
version: 1
rules:
  - name: no-console-log
    description: Remove console.log statements
    message: "Remove console.log before committing."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (call_expression
                function: (member_expression
                  object: (identifier) @obj
                  property: (property_identifier) @prop)
                (#eq? @obj "console")
                (#eq? @prop "log"))
"#;

    let dir = setup_config(config);

    // This should pass: no console.log
    let input = write_hook("test.ts", "function greet(name: string): string {\n    return `Hello, ${name}`;\n}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough without console.log, got: {output}"
    );
}

#[test]
fn test_typescript_syntax_tree_with_capture_interpolation() {
    let config = r#"
version: 1
rules:
  - name: function-name-check
    description: Check function naming in TypeScript
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(function_declaration name: (identifier) @fn_name)"
            suggestion: "Found TypeScript function named `{{ $fn_name }}`"
"#;

    let dir = setup_config(config);

    let input = write_hook("test.ts", "function myTypeScriptFunction(): void {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);

    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains("myTypeScriptFunction"),
        "expected suggestion to contain function name, got: {output}"
    );
}

// =============================================================================
// Category 1: Import/Export Patterns
// =============================================================================

#[test]
fn test_typescript_no_wildcard_imports() {
    // Matches: import * as foo from 'bar'
    let config = r#"
version: 1
rules:
  - name: no-wildcard-imports
    description: Avoid wildcard imports for better tree-shaking
    message: "Avoid wildcard imports. Import specific exports instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(import_statement (import_clause (namespace_import)) @wildcard)"
"#;

    let dir = setup_config(config);

    // Should trigger: wildcard import
    let input = write_hook("test.ts", "import * as utils from './utils';");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for wildcard import, got: {output}"
    );

    // Should pass: named import
    let input = write_hook("test.ts", "import { foo, bar } from './utils';");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for named import, got: {output}"
    );
}

#[test]
fn test_typescript_no_default_export() {
    // Matches: export default ...
    let config = r#"
version: 1
rules:
  - name: no-default-export
    description: Use named exports for better refactoring support
    message: "Use named exports instead of default exports."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(export_statement \"default\" @default)"
"#;

    let dir = setup_config(config);

    // Should trigger: default export
    let input = write_hook("test.ts", "export default function greet() {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for default export, got: {output}"
    );

    // Should pass: named export
    let input = write_hook("test.ts", "export function greet() {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for named export, got: {output}"
    );
}

#[test]
fn test_typescript_no_relative_parent_imports() {
    // Matches: import from '../' paths
    let config = r#"
version: 1
rules:
  - name: no-parent-imports
    description: Avoid deep relative imports
    message: "Avoid '../' imports. Use path aliases instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (import_statement
                source: (string (string_fragment) @path)
                (#match? @path "^\\.\\."))
"#;

    let dir = setup_config(config);

    // Should trigger: parent directory import
    let input = write_hook("test.ts", "import { foo } from '../utils';");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for parent import, got: {output}"
    );

    // Should pass: same-directory import
    let input = write_hook("test.ts", "import { foo } from './utils';");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for local import, got: {output}"
    );
}

#[test]
fn test_typescript_no_dynamic_imports() {
    // Matches: import('module') - dynamic imports
    let config = r#"
version: 1
rules:
  - name: no-dynamic-imports
    description: Avoid dynamic imports for better static analysis
    message: "Avoid dynamic imports. Use static imports instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(call_expression function: (import)) @dynamic_import"
"#;

    let dir = setup_config(config);

    // Should trigger: dynamic import
    let input = write_hook("test.ts", "const module = await import('./utils');");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for dynamic import, got: {output}"
    );

    // Should pass: static import
    let input = write_hook("test.ts", "import { foo } from './utils';");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for static import, got: {output}"
    );
}

// =============================================================================
// Category 2: Function Patterns
// =============================================================================

#[test]
fn test_typescript_no_nested_functions() {
    // Matches: function declarations inside function bodies
    let config = r#"
version: 1
rules:
  - name: no-nested-functions
    description: Avoid nested function declarations
    message: "Extract nested functions to module scope or use arrow functions."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (function_declaration
                body: (statement_block
                  (function_declaration) @nested))
"#;

    let dir = setup_config(config);

    // Should trigger: nested function declaration
    let input = write_hook(
        "test.ts",
        r#"function outer() {
    function inner() {
        return 42;
    }
    return inner();
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for nested function, got: {output}"
    );

    // Should pass: arrow function inside (not a declaration)
    let input = write_hook(
        "test.ts",
        r#"function outer() {
    const inner = () => 42;
    return inner();
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for arrow function, got: {output}"
    );
}

#[test]
fn test_typescript_arrow_in_class_property() {
    // Matches: class properties initialized with arrow functions
    let config = r#"
version: 1
rules:
  - name: arrow-in-class-property
    description: Detect arrow functions in class properties (potential this-binding issue)
    message: "Consider using a method instead of arrow function property for {{ $prop }}."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (public_field_definition
                name: (property_identifier) @prop
                value: (arrow_function)) @field
"#;

    let dir = setup_config(config);

    // Should trigger: arrow function in class property
    let input = write_hook(
        "test.ts",
        r#"class Button {
    onClick = () => {
        console.log('clicked');
    };
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for arrow in class property, got: {output}"
    );
    assert!(
        output.contains("onClick"),
        "expected property name in output, got: {output}"
    );

    // Should pass: regular method
    let input = write_hook(
        "test.ts",
        r#"class Button {
    onClick() {
        console.log('clicked');
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for method, got: {output}"
    );
}

#[test]
fn test_typescript_no_generator_functions() {
    // Matches: generator functions (function*)
    let config = r#"
version: 1
rules:
  - name: no-generators
    description: Prefer async/await over generators
    message: "Avoid generator functions. Use async/await instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(generator_function_declaration) @generator"
"#;

    let dir = setup_config(config);

    // Should trigger: generator function
    let input = write_hook(
        "test.ts",
        r#"function* idGenerator() {
    let id = 0;
    while (true) {
        yield id++;
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for generator, got: {output}"
    );

    // Should pass: async function
    let input = write_hook(
        "test.ts",
        r#"async function fetchData() {
    return await fetch('/api');
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for async function, got: {output}"
    );
}

#[test]
fn test_typescript_no_callback_functions() {
    // Matches: function expressions passed as arguments (callbacks)
    let config = r#"
version: 1
rules:
  - name: no-function-callbacks
    description: Prefer arrow functions for callbacks
    message: "Use arrow functions for callbacks instead of function expressions."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(arguments (function_expression) @callback)"
"#;

    let dir = setup_config(config);

    // Should trigger: function expression as callback
    let input = write_hook(
        "test.ts",
        r#"arr.map(function(x) {
    return x * 2;
});"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for function callback, got: {output}"
    );

    // Should pass: arrow function callback
    let input = write_hook("test.ts", "arr.map((x) => x * 2);");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for arrow callback, got: {output}"
    );
}

// =============================================================================
// Category 3: Type Safety
// =============================================================================

#[test]
fn test_typescript_no_any_type() {
    // Matches: any type annotation
    let config = r#"
version: 1
rules:
  - name: no-any-type
    description: Avoid using 'any' type
    message: "Don't use 'any'. Use 'unknown' or a specific type instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (type_annotation
                (predefined_type) @type
                (#eq? @type "any"))
"#;

    let dir = setup_config(config);

    // Should trigger: any type annotation
    let input = write_hook("test.ts", "function process(data: any): void {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for any type, got: {output}"
    );

    // Should pass: specific type
    let input = write_hook("test.ts", "function process(data: string): void {}");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for specific type, got: {output}"
    );
}

#[test]
fn test_typescript_no_type_assertions() {
    // Matches: type assertions (as Type)
    let config = r#"
version: 1
rules:
  - name: no-type-assertions
    description: Avoid type assertions
    message: "Avoid type assertions. Use type guards or proper typing instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(as_expression) @assertion"
"#;

    let dir = setup_config(config);

    // Should trigger: type assertion
    let input = write_hook("test.ts", "const el = document.getElementById('app') as HTMLDivElement;");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for type assertion, got: {output}"
    );

    // Should pass: no assertion
    let input = write_hook("test.ts", "const el: HTMLDivElement | null = document.getElementById('app');");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough without assertion, got: {output}"
    );
}

#[test]
fn test_typescript_no_non_null_assertions() {
    // Matches: non-null assertions (value!)
    let config = r#"
version: 1
rules:
  - name: no-non-null-assertions
    description: Avoid non-null assertions
    message: "Avoid non-null assertions. Use proper null checks instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: "(non_null_expression) @assertion"
"#;

    let dir = setup_config(config);

    // Should trigger: non-null assertion
    let input = write_hook("test.ts", "const name = user.profile!.name;");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for non-null assertion, got: {output}"
    );

    // Should pass: optional chaining
    let input = write_hook("test.ts", "const name = user.profile?.name;");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for optional chaining, got: {output}"
    );
}

#[test]
fn test_typescript_deep_member_access() {
    // Matches: deeply nested member access (4+ levels) - potential code smell
    let config = r#"
version: 1
rules:
  - name: deep-member-access
    description: Avoid deeply nested property access
    message: "Deeply nested property access suggests Law of Demeter violation."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.ts"
        content:
          - kind: SyntaxTree
            language: typescript
            query: |
              (member_expression
                object: (member_expression
                  object: (member_expression
                    object: (member_expression)))) @deep
"#;

    let dir = setup_config(config);

    // Should trigger: 4+ levels of property access
    let input = write_hook("test.ts", "const x = a.b.c.d.e;");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for deep access, got: {output}"
    );

    // Should pass: shallow property access
    let input = write_hook("test.ts", "const x = a.b.c;");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for shallow access, got: {output}"
    );
}
