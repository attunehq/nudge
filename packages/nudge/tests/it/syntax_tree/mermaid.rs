//! Mermaid syntax tree tests.

use std::fs;

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::{run_hook_in_dir, run_nudge_in_dir, setup_config, write_hook};

#[test]
fn test_mermaid_flowchart_write_matches_standalone_mmd() {
    let config = mermaid_flowchart_config();
    let dir = setup_config(config);

    let input = write_hook("diagram.mmd", "flowchart TD\n  Start --> Done\n");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0, output: {output}");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected interrupt for Mermaid flow edge, got: {output}"
    );
    assert!(
        output.contains("Review Mermaid flow edge `Start --> Done`."),
        "expected Mermaid capture interpolation, got: {output}"
    );

    let input = write_hook("diagram.mmd", "This is not a Mermaid diagram.\n");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for non-Mermaid content, got: {output}"
    );

    let input = write_hook("diagram.md", "flowchart TD\n  Start --> Done\n");
    let (exit_code, output) = run_hook_in_dir(&dir, &input);
    pretty_assert_eq!(exit_code, 0, "expected exit 0");
    assert!(
        output.is_empty(),
        "expected passthrough for Markdown because this thread only supports standalone Mermaid files, got: {output}"
    );
}

#[test]
fn test_mermaid_check_scans_mmd_and_mermaid_files() {
    let config = mermaid_flowchart_config();
    let dir = setup_config(config);
    let diagrams = dir.path().join("diagrams");
    fs::create_dir(&diagrams).expect("create diagrams directory");
    fs::write(diagrams.join("bad.mmd"), "flowchart TD\n  Start --> Done\n")
        .expect("write .mmd fixture");
    fs::write(
        diagrams.join("bad.mermaid"),
        "flowchart TD\n  Login --> Dashboard\n",
    )
    .expect("write .mermaid fixture");
    fs::write(
        diagrams.join("notes.md"),
        "flowchart TD\n  Hidden --> Ignored\n",
    )
    .expect("write Markdown fixture");
    fs::write(
        diagrams.join("safe.mmd"),
        "sequenceDiagram\n  participant User\n",
    )
    .expect("write safe Mermaid fixture");

    let (exit_code, stdout, stderr) = run_nudge_in_dir(&dir, &["check", "diagrams"]);
    pretty_assert_eq!(
        exit_code,
        1,
        "expected check to report Mermaid issues, stdout: {stdout}, stderr: {stderr}"
    );
    assert!(
        stdout.contains("diagrams/bad.mmd:2 [review-mermaid-flow]"),
        "expected check to report .mmd flow edge, got: {stdout}"
    );
    assert!(
        stdout.contains("Review Mermaid flow edge `Start --> Done`."),
        "expected check message for .mmd capture, got: {stdout}"
    );
    assert!(
        stdout.contains("diagrams/bad.mermaid:2 [review-mermaid-flow]"),
        "expected check to report .mermaid flow edge, got: {stdout}"
    );
    assert!(
        stdout.contains("Review Mermaid flow edge `Login --> Dashboard`."),
        "expected check message for .mermaid capture, got: {stdout}"
    );
    assert!(
        !stdout.contains("notes.md") && !stdout.contains("safe.mmd"),
        "expected check to skip Markdown and non-flow Mermaid files, got: {stdout}"
    );
}

fn mermaid_flowchart_config() -> &'static str {
    r#"
version: 1
rules:
  - name: review-mermaid-flow
    description: Review Mermaid flowchart edges
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.mmd"
        content:
          - kind: SyntaxTree
            language: mermaid
            query: "(flow_stmt_vertice) @edge"
            suggestion: "Review Mermaid flow edge `{{ $edge }}`."
      - hook: PreToolUse
        tool: Write
        file: "**/*.mermaid"
        content:
          - kind: SyntaxTree
            language: mermaid
            query: "(flow_stmt_vertice) @edge"
            suggestion: "Review Mermaid flow edge `{{ $edge }}`."
"#
}
