//! Java syntax tree tests.

use std::fs;

use pretty_assertions::assert_eq as pretty_assert_eq;

use crate::edit_hook;

use super::{run_hook_in_dir, run_nudge_in_dir, setup_config, write_hook};

fn assert_denied(output: &str, context: &str) {
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for {context}, got: {output}"
    );
}

fn assert_passthrough(output: &str, context: &str) {
    assert!(
        output.is_empty(),
        "expected passthrough for {context}, got: {output}"
    );
}

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
    assert_denied(&output, "generic Exception");

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
    assert_passthrough(&output, "specific exception");
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
    assert_denied(&output, "System.out.println");

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
    assert_passthrough(&output, "logger");

    // Should pass: text mentions are comments/strings, not Java method calls.
    let input = write_hook(
        "Test.java",
        r#"public class Test {
    public void example() {
        // System.out.println("debug");
        String code = "System.out.println(\"debug\")";
        logger.info("debug");
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert_passthrough(
        &output,
        "comments and strings mentioning System.out.println",
    );
}

#[test]
fn test_java_synchronized_method_suggestion_interpolation_and_span() {
    let config = r#"
version: 1
rules:
  - name: no-synchronized-methods
    description: Prefer narrower locks over synchronized methods
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.java"
        content:
          - kind: SyntaxTree
            language: java
            query: |
              (method_declaration
                (modifiers "synchronized" @modifier)
                name: (identifier) @method)
            suggestion: "Method `{{ $method }}` uses `{{ $modifier }}`; prefer a narrower lock, then retry."
"#;

    let dir = setup_config(config);

    let input = write_hook(
        "Worker.java",
        r#"public class Worker {
    @Deprecated
    public synchronized void flush() {
        drain();
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_denied(&output, "synchronized method");
    assert!(
        output.contains("Method `flush` uses `synchronized`"),
        "expected capture and suggestion interpolation, got: {output}"
    );
    assert!(
        output.contains("public synchronized void flush"),
        "expected snippet span around synchronized method, got: {output}"
    );

    let input = write_hook(
        "Worker.java",
        r#"public class Worker {
    public void flush() {
        synchronized (this) {
            drain();
        }
    }
}"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert_passthrough(&output, "synchronized block inside non-synchronized method");
}

#[test]
fn test_java_edit_syntax_tree_matches_new_content() {
    let config = r#"
version: 1
rules:
  - name: no-deprecated-annotation
    description: Do not add Deprecated annotations
    message: "Remove @{{ $annotation }} from Java edits, then retry."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.java"
        new_content:
          - kind: SyntaxTree
            language: java
            query: |
              (marker_annotation
                name: (identifier) @annotation
                (#eq? @annotation "Deprecated"))
"#;

    let dir = setup_config(config);

    let input = edit_hook(
        "Worker.java",
        "public void flush() {}",
        "@Deprecated\npublic void flush() {}",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_denied(&output, "Deprecated annotation in Edit new_content");
    assert!(
        output.contains("Remove @Deprecated"),
        "expected annotation capture interpolation, got: {output}"
    );

    let input = edit_hook(
        "Worker.java",
        "public void flush() {}",
        "@Override\npublic void flush() {}",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert_passthrough(&output, "non-matching annotation in Edit new_content");
}

#[test]
fn test_java_check_scans_write_and_edit_rules() {
    let config = r#"
version: 1
rules:
  - name: no-raw-list-field
    description: Prefer parameterized List fields
    message: "Parameterize raw List field `{{ $field }}`, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.java"
        content:
          - kind: SyntaxTree
            language: java
            query: |
              (field_declaration
                type: (type_identifier) @type
                declarator: (variable_declarator
                  name: (identifier) @field)
                (#eq? @type "List"))
  - name: no-system-out-edit
    description: Do not add System.out.println in edits
    message: "Use logger instead of System.out.{{ $method }}, then retry."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.java"
        new_content:
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
    fs::write(
        dir.path().join("Unsafe.java"),
        "import java.util.List;\nclass Unsafe { List names; }\n",
    )
    .expect("write unsafe java fixture");
    fs::write(
        dir.path().join("Debug.java"),
        "class Debug { void run() { System.out.println(\"debug\"); } }\n",
    )
    .expect("write debug java fixture");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check"]);
    assert_ne!(
        exit_code, 0,
        "expected nudge check to fail for Java violations; stdout: {stdout}; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Unsafe.java") && stdout.contains("Parameterize raw List field `names`"),
        "expected raw List issue from Write rule, got: {stdout}"
    );
    assert!(
        stdout.contains("Debug.java")
            && stdout.contains("Use logger instead of System.out.println"),
        "expected System.out issue from Edit rule, got: {stdout}"
    );

    fs::write(
        dir.path().join("Unsafe.java"),
        "import java.util.List;\nclass Unsafe { List<String> names; }\n",
    )
    .expect("write safe java fixture");
    fs::write(
        dir.path().join("Debug.java"),
        "class Debug { void run() { logger.info(\"debug\"); } }\n",
    )
    .expect("write safe debug fixture");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check"]);
    pretty_assert_eq!(
        exit_code,
        0,
        "expected nudge check to pass for safe Java fixtures; stdout: {stdout}; stderr: {stderr}"
    );
    assert!(
        stdout.contains("Checked 2 files against 2 rules"),
        "expected successful Java check summary, got: {stdout}"
    );
}

#[test]
fn test_java_incomplete_code_uses_recoverable_parse_tree() {
    let config = r#"
version: 1
rules:
  - name: no-system-out-in-incomplete-java
    description: Do not use System.out.println
    message: "Use logger instead of System.out.{{ $method }}, then retry."
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

    let input = write_hook(
        "Broken.java",
        r#"class Broken { void run() { System.out.println("debug") "#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert_denied(&output, "recoverable incomplete Java System.out.println");
    assert!(
        output.contains("System.out.println"),
        "expected incomplete Java snippet to include method invocation, got: {output}"
    );

    let input = write_hook(
        "Broken.java",
        r#"class Broken { void run() { String text = "System.out.println(\"debug\")" "#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert_passthrough(&output, "incomplete Java string mention");
}

#[test]
fn test_java_syntaxtree_cli_shows_java_nodes_and_fields() {
    let dir = setup_config("version: 1\nrules: []\n");
    let (exit_code, stdout, stderr) = run_nudge_in_dir(
        &dir,
        &[
            "syntaxtree",
            "--language",
            "java",
            "class Test { @Override synchronized void work() {} }",
        ],
    );

    pretty_assert_eq!(
        exit_code,
        0,
        "expected syntaxtree java to exit 0; stdout: {stdout}; stderr: {stderr}"
    );
    assert!(
        stdout.contains("class_declaration") && stdout.contains("method_declaration"),
        "expected Java syntax tree nodes, got: {stdout}"
    );
    assert!(
        stdout.contains("marker_annotation")
            && stdout.contains("synchronized")
            && stdout.contains("name:"),
        "expected Java annotations, modifier, and field names, got: {stdout}"
    );
}
