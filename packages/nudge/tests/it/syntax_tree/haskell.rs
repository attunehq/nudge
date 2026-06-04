//! Haskell syntax tree tests.

use std::fs;

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{edit_hook, run_hook_in_dir, run_nudge_in_dir, setup_config, write_hook};

#[test]
fn test_haskell_top_level_function_with_suggestion_interpolation() {
    // Matches top-level functions and captures the function name for message
    // interpolation.
    let config = r#"
version: 1
rules:
  - name: function-definition
    description: Match function definitions
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.hs"
        content:
          - kind: SyntaxTree
            language: haskell
            query: "(function name: (variable) @fn_name)"
            suggestion: "Found Haskell function `{{ $fn_name }}`."
"#;

    let dir = setup_config(config);

    // Should trigger: function with arguments.
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
        "expected interrupt for function, got: {output}"
    );
    assert!(
        output.contains("Found Haskell function `firstElem`."),
        "expected captured function name in suggestion, got: {output}"
    );

    // Should pass: a type signature only is a `signature`, not a function body.
    let input = write_hook(
        "Test.hs",
        r#"module Main where

firstElem :: [a] -> a"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for type sig only, got: {output}"
    );
}

#[test]
fn test_haskell_top_level_bind() {
    let config = r#"
version: 1
rules:
  - name: top-level-bind
    description: Match top-level Haskell binds
    message: "Found top-level bind `{{ $bind_name }}`."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.hs"
        content:
          - kind: SyntaxTree
            language: haskell
            query: "(bind name: (variable) @bind_name)"
"#;

    let dir = setup_config(config);

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
        "expected interrupt for top-level bind, got: {output}"
    );
    assert!(
        output.contains("Found top-level bind `main`."),
        "expected bind capture in message, got: {output}"
    );

    let input = write_hook(
        "Test.hs",
        r#"module Main where

main :: IO ()"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for signature without bind, got: {output}"
    );
}

#[test]
fn test_haskell_head_function_write_and_false_positives() {
    // Matches: use of partial function `head`.
    let config = r#"
version: 1
rules:
  - name: no-head
    description: Avoid partial function head
    message: "{{ $suggestion }}"
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
                argument: (_) @arg
                (#eq? @fn "head"))
            suggestion: "Avoid `{{ $fn }} {{ $arg }}`. Use pattern matching or `listToMaybe` instead."
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
    assert!(
        output.contains("Avoid `head xs`."),
        "expected capture text interpolation for head call, got: {output}"
    );
    assert!(
        output.contains("firstElem xs = head xs"),
        "expected snippet to include the Haskell source line, got: {output}"
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

    // Should pass: mentions in comments and strings are not `apply` nodes.
    let input = write_hook(
        "Test.hs",
        r#"module Main where

note :: String
note = "head xs is unsafe"

-- head xs is also only a comment here
safeHead :: [a] -> Maybe a
safeHead [] = Nothing
safeHead (x:_) = Just x"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for comments and strings, got: {output}"
    );
}

#[test]
fn test_haskell_head_function_edit() {
    let config = r#"
version: 1
rules:
  - name: no-head-edit
    description: Avoid partial function head in edits
    message: "Avoid `head`. Use pattern matching or `listToMaybe` instead."
    on:
      - hook: PreToolUse
        tool: Edit
        file: "**/*.hs"
        new_content:
          - kind: SyntaxTree
            language: haskell
            query: |
              (apply
                function: (variable) @fn
                (#eq? @fn "head"))
"#;

    let dir = setup_config(config);

    let input = edit_hook(
        "Test.hs",
        "firstElem xs = listToMaybe xs",
        "firstElem xs = head xs",
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for Edit new_content with head, got: {output}"
    );

    let input = edit_hook(
        "Test.hs",
        "firstElem xs = head xs",
        r#"firstElem [] = Nothing
firstElem (x:_) = Just x"#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for safe Edit new_content, got: {output}"
    );
}

#[test]
fn test_haskell_check_scans_files_and_interpolates_import_captures() {
    let config = r#"
version: 1
rules:
  - name: no-unsafe-perform-io
    description: Avoid importing unsafePerformIO
    message: "Avoid importing {{ $name }} from {{ $module }}."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.hs"
        content:
          - kind: SyntaxTree
            language: haskell
            query: |
              (import
                module: (module) @module
                names: (import_list
                  (import_name
                    variable: (variable) @name))
                (#eq? @module "System.IO.Unsafe")
                (#eq? @name "unsafePerformIO"))
"#;

    let dir = setup_config(config);
    fs::write(
        dir.path().join("UnsafeExample.hs"),
        r#"module UnsafeExample where

import System.IO.Unsafe (unsafePerformIO)

value :: Int
value = unsafePerformIO (pure 1)
"#,
    )
    .expect("write unsafe Haskell fixture");

    fs::write(
        dir.path().join("SafeExample.hs"),
        r#"module SafeExample where

import Data.Maybe (listToMaybe)

value :: Maybe Int
value = listToMaybe [1]
"#,
    )
    .expect("write safe Haskell fixture");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check"]);
    assert_ne!(
        exit_code, 0,
        "expected check to fail for unsafe import, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("UnsafeExample.hs:3"),
        "expected check output to include unsafe import line, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("Avoid importing unsafePerformIO from System.IO.Unsafe."),
        "expected check output to interpolate Haskell captures, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        !stdout.contains("SafeExample.hs"),
        "expected safe Haskell file not to report, stdout: {stdout}, stderr: {stderr}"
    );
}

#[test]
fn test_haskell_incomplete_code_uses_existing_error_tree_policy() {
    let config = r#"
version: 1
rules:
  - name: no-head-in-incomplete-code
    description: Avoid partial function head
    message: "Avoid `head`."
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

    // Incomplete Haskell should not fail parsing the hook. Like the other
    // languages, Nudge accepts error trees and simply returns no match when
    // the queried structure is absent.
    let input = write_hook(
        "Test.hs",
        r#"module Main where

firstElem xs ="#,
    );
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for incomplete code with no matched apply, got: {output}"
    );
}

#[test]
fn test_haskell_syntaxtree_command() {
    let dir = setup_config("version: 1\nrules: []\n");
    let (exit_code, stdout, stderr) = run_nudge_in_dir(
        &dir,
        &[
            "syntaxtree",
            "--language",
            "haskell",
            "firstElem xs = head xs",
        ],
    );

    pretty_assert_eq!(exit_code, 0, "syntaxtree should exit 0, stderr: {stderr}");
    assert!(
        stdout.contains("function"),
        "syntaxtree should show Haskell function nodes, got: {stdout}"
    );
    assert!(
        stdout.contains("function:") && stdout.contains("head"),
        "syntaxtree should show Haskell field names and head capture target, got: {stdout}"
    );
}
