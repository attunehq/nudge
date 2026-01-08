//! TypeScript type safety tests (Category 3).

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

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
    let input = write_hook(
        "test.ts",
        "const el = document.getElementById('app') as HTMLDivElement;",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for type assertion, got: {output}"
    );

    // Should pass: no assertion
    let input = write_hook(
        "test.ts",
        "const el: HTMLDivElement | null = document.getElementById('app');",
    );
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
