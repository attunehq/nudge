//! C# syntax tree tests.

use std::fs;
use std::process::Command;

use crate::{edit_hook, nudge_binary};
use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

#[test]
fn test_csharp_empty_catch_block() {
    // Matches: empty catch blocks
    let config = r#"
version: 1
rules:
  - name: no-empty-catch
    description: Don't use empty catch blocks
    message: "Empty catch block. At minimum, log the exception."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.cs"
        content:
          - kind: SyntaxTree
            language: csharp
            query: |
              (catch_clause
                body: (block) @body
                (#eq? @body "{ }"))
"#;

    let dir = setup_config(config);

    // Should trigger: empty catch block
    let input = write_hook(
        "Test.cs",
        r#"public class Test {
    public void Example() {
        try {
            RiskyOperation();
        } catch (Exception e) { }
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for empty catch, got: {output}"
    );

    // Should pass: catch with logging
    let input = write_hook(
        "Test.cs",
        r#"public class Test {
    public void Example() {
        try {
            RiskyOperation();
        } catch (Exception e) {
            Console.WriteLine(e);
        }
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for non-empty catch, got: {output}"
    );
}

#[test]
fn test_csharp_console_writeline() {
    // Matches: Console.WriteLine() calls
    let config = r#"
version: 1
rules:
  - name: no-console-writeline
    description: Don't use Console.WriteLine in production code
    message: "Use a logging framework instead of `Console.WriteLine`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.cs"
        content:
          - kind: SyntaxTree
            language: csharp
            query: |
              (invocation_expression
                function: (member_access_expression
                  expression: (identifier) @obj
                  name: (identifier) @method)
                (#eq? @obj "Console")
                (#eq? @method "WriteLine"))
"#;

    let dir = setup_config(config);

    // Should trigger: Console.WriteLine
    let input = write_hook(
        "Test.cs",
        r#"public class Test {
    public void Example() {
        Console.WriteLine("debug");
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for Console.WriteLine, got: {output}"
    );

    // Should pass: logger call
    let input = write_hook(
        "Test.cs",
        r#"public class Test {
    private readonly ILogger _logger;
    public void Example() {
        _logger.LogInformation("debug");
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for logger, got: {output}"
    );

    // Should pass: comments and strings mentioning Console.WriteLine are not
    // invocations.
    let input = write_hook(
        "Test.cs",
        r#"public class Test {
    public void Example() {
        // Console.WriteLine("debug");
        var text = "Console.WriteLine(\"debug\")";
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for comments and strings, got: {output}"
    );
}

#[test]
fn test_csharp_async_void_edit() {
    let config = r#"
version: 1
rules:
  - name: no-async-void
    description: Use Task-returning async methods except for event handlers
    message: "Use `Task` instead of `async void` for `{{ $method }}`, then retry."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.cs"
        new_content:
          - kind: SyntaxTree
            language: csharp
            query: |
              (method_declaration
                (modifier) @modifier
                returns: (predefined_type) @return_type
                name: (identifier) @method
                (#eq? @modifier "async")
                (#eq? @return_type "void"))
"#;

    let dir = setup_config(config);

    let input = edit_hook(
        "Test.cs",
        "old code",
        r#"public class Test {
    public async void SaveAsync() {
        await Task.Delay(1);
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#) && output.contains("SaveAsync"),
        "expected interrupt with captured method name, got: {output}"
    );

    let input = edit_hook(
        "Test.cs",
        "old code",
        r#"public class Test {
    public async Task SaveAsync() {
        await Task.Delay(1);
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for async Task, got: {output}"
    );
}

#[test]
fn test_csharp_nullable_capture_and_suggestion_interpolation() {
    let config = r#"
version: 1
rules:
  - name: nullable-property-review
    description: Review nullable public properties
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.cs"
        content:
          - kind: SyntaxTree
            language: csharp
            query: |
              (property_declaration
                type: (nullable_type) @type
                name: (identifier) @property)
            suggestion: "Review nullable property `{{ $property }}` of type `{{ $type }}`."
"#;

    let dir = setup_config(config);

    let input = write_hook(
        "Test.cs",
        r#"public class Test {
    public string? DisplayName { get; set; }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#)
            && output.contains("DisplayName")
            && output.contains("string?"),
        "expected interpolated nullable property capture, got: {output}"
    );

    let input = write_hook(
        "Test.cs",
        r#"public class Test {
    public string DisplayName { get; set; }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for non-nullable property, got: {output}"
    );
}

#[test]
fn test_csharp_incomplete_code_uses_partial_tree() {
    let config = r#"
version: 1
rules:
  - name: no-console-writeline
    description: Don't use Console.WriteLine in production code
    message: "Use a logging framework instead of `Console.WriteLine`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.cs"
        content:
          - kind: SyntaxTree
            language: csharp
            query: |
              (invocation_expression
                function: (member_access_expression
                  expression: (identifier) @obj
                  name: (identifier) @method)
                (#eq? @obj "Console")
                (#eq? @method "WriteLine"))
"#;

    let dir = setup_config(config);

    let input = write_hook(
        "Test.cs",
        r#"public class Test {
    public void Example() {
        Console.WriteLine("debug");
"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected partial C# tree to still expose complete invocation, got: {output}"
    );
}

#[test]
fn test_csharp_check_scans_write_and_edit_rules() {
    let config = r#"
version: 1
rules:
  - name: no-console-writeline
    description: Don't use Console.WriteLine in production code
    message: "Use a logging framework instead of `Console.WriteLine`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.cs"
        content:
          - kind: SyntaxTree
            language: csharp
            query: |
              (invocation_expression
                function: (member_access_expression
                  expression: (identifier) @obj
                  name: (identifier) @method)
                (#eq? @obj "Console")
                (#eq? @method "WriteLine"))
  - name: no-async-void
    description: Use Task-returning async methods except for event handlers
    message: "Use `Task` instead of `async void` for `{{ $method }}`, then retry."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.cs"
        new_content:
          - kind: SyntaxTree
            language: csharp
            query: |
              (method_declaration
                (modifier) @modifier
                returns: (predefined_type) @return_type
                name: (identifier) @method
                (#eq? @modifier "async")
                (#eq? @return_type "void"))
"#;

    let dir = setup_config(config);
    fs::write(
        dir.path().join("Unsafe.cs"),
        r#"public class Unsafe {
    public async void SaveAsync() {
        await Task.Delay(1);
    }

    public void Example() {
        Console.WriteLine("debug");
    }
}"#,
    )
    .expect("write unsafe C# file");
    fs::write(
        dir.path().join("Safe.cs"),
        r#"public class Safe {
    public async Task SaveAsync() {
        await Task.Delay(1);
    }

    public void Example(ILogger logger) {
        logger.LogInformation("debug");
    }
}"#,
    )
    .expect("write safe C# file");

    let output = Command::new(nudge_binary())
        .arg("check")
        .arg("Unsafe.cs")
        .current_dir(dir.path())
        .output()
        .expect("run nudge check on unsafe file");
    pretty_assert_eq!(output.status.code(), Some(1), "check should fail");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Unsafe.cs")
            && stdout.contains("no-console-writeline")
            && stdout.contains("no-async-void")
            && stdout.contains("SaveAsync"),
        "expected check to report both C# SyntaxTree rules, got: {stdout}"
    );

    let output = Command::new(nudge_binary())
        .arg("check")
        .arg("Safe.cs")
        .current_dir(dir.path())
        .output()
        .expect("run nudge check on safe file");
    pretty_assert_eq!(output.status.code(), Some(0), "check should pass");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Checked 1 files"),
        "expected check to scan the safe C# file, got: {stdout}"
    );
}
