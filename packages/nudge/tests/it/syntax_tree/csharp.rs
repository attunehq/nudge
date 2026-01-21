//! C# syntax tree tests.

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
}
