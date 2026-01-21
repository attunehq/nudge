//! Haskell syntax tree tests.

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, setup_config, write_hook};

#[test]
fn test_haskell_function_parsing() {
    // Basic test: verify Haskell parsing works by matching bind definitions
    // In tree-sitter-haskell, simple definitions without patterns are `bind` nodes
    let config = r#"
version: 1
rules:
  - name: function-definition
    description: Match function definitions
    message: "Found function definition."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.hs"
        content:
          - kind: SyntaxTree
            language: haskell
            query: "(bind) @fn"
"#;

    let dir = setup_config(config);

    // Should trigger: bind definition (main = ...)
    let input = write_hook(
        "Test.hs",
        r#"module Main where

main :: IO ()
main = putStrLn "Hello""#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for function, got: {output}"
    );

    // Should pass: just a type signature (no function body)
    let input = write_hook(
        "Test.hs",
        r#"module Main where

main :: IO ()"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for type sig only, got: {output}"
    );
}

#[test]
fn test_haskell_head_function() {
    // Matches: use of partial function `head`
    // In tree-sitter-haskell, function references are `variable` nodes
    let config = r#"
version: 1
rules:
  - name: no-head
    description: Avoid partial function head
    message: "Avoid `head`. Use pattern matching or `listToMaybe` instead."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.hs"
        content:
          - kind: SyntaxTree
            language: haskell
            query: |
              (apply
                function: (variable) @fn
                (#eq? @fn "head"))
"#;

    let dir = setup_config(config);

    // Should trigger: use of head
    let input = write_hook(
        "Test.hs",
        r#"module Main where

firstElem :: [a] -> a
firstElem xs = head xs"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for head, got: {output}"
    );

    // Should pass: pattern matching
    let input = write_hook(
        "Test.hs",
        r#"module Main where

firstElem :: [a] -> Maybe a
firstElem [] = Nothing
firstElem (x:_) = Just x"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for pattern matching, got: {output}"
    );
}
