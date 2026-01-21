//! Java syntax tree tests.

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

#[test]
fn test_java_catch_generic_exception() {
    // Matches: catch blocks catching generic Exception
    let config = r#"
version: 1
rules:
  - name: no-catch-generic-exception
    description: Don't catch generic Exception
    message: "Catch specific exceptions instead of generic `Exception`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.java"
        content:
          - kind: SyntaxTree
            language: java
            query: |
              (catch_clause
                (catch_formal_parameter
                  (catch_type (type_identifier) @type))
                (#eq? @type "Exception"))
"#;

    let dir = setup_config(config);

    // Should trigger: catching generic Exception
    let input = write_hook(
        "Test.java",
        r#"public class Test {
    public void example() {
        try {
            riskyOperation();
        } catch (Exception e) {
            e.printStackTrace();
        }
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for generic Exception, got: {output}"
    );

    // Should pass: catching specific exception
    let input = write_hook(
        "Test.java",
        r#"public class Test {
    public void example() {
        try {
            riskyOperation();
        } catch (IOException e) {
            e.printStackTrace();
        }
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for specific exception, got: {output}"
    );
}

#[test]
fn test_java_system_out_println() {
    // Matches: System.out.println() calls
    let config = r#"
version: 1
rules:
  - name: no-system-out
    description: Don't use System.out.println
    message: "Use a logging framework instead of `System.out.println`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.java"
        content:
          - kind: SyntaxTree
            language: java
            query: |
              (method_invocation
                object: (field_access
                  object: (identifier) @obj
                  field: (identifier) @field)
                name: (identifier) @method
                (#eq? @obj "System")
                (#eq? @field "out")
                (#eq? @method "println"))
"#;

    let dir = setup_config(config);

    // Should trigger: System.out.println
    let input = write_hook(
        "Test.java",
        r#"public class Test {
    public void example() {
        System.out.println("debug");
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for System.out.println, got: {output}"
    );

    // Should pass: logger call
    let input = write_hook(
        "Test.java",
        r#"public class Test {
    private static final Logger logger = LoggerFactory.getLogger(Test.class);
    public void example() {
        logger.info("debug");
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
