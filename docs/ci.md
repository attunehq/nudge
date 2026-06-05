# CI and Programmatic Checks

`nudge check` runs Nudge as a one-shot project checker. It does not require
Claude Code, Codex CLI, hook installation, or a live agent session. Use it for
CI jobs, local pre-commit checks, release gates, or any script that needs a
plain command with a reliable exit status.

## Quick Start

From a project with Nudge rules:

```bash
# Check the whole project from the current directory.
nudge check

# Check specific directories, files, or glob patterns.
nudge check src/ docs/
nudge check src/lib.rs
nudge check "**/*.rs"
```

Exit behavior:

- `0`: no checkable violations were found, or no file-based rules exist.
- `1`: one or more checkable violations were found.
- Other non-zero exits: configuration, argument, or runtime errors.

Example failure output:

```text
x Found 2 issues in 1 file

src/lib.rs:42 [no-unwrap]
  Use `.expect("descriptive error message")` instead of `.unwrap()`, then retry.

src/lib.rs:57 [no-inline-imports]
  Move this `use` statement to the top of the file, then retry.

Checked 25 files against 6 rules
```

## Rule Discovery

`nudge check` loads the same rule files as hook mode:

1. User-level `rules.yaml` from Nudge's platform config directory, if present.
2. Project `.nudge.yaml`, if present.
3. Every YAML file under project `.nudge/`, walked recursively in file-name
   order.

Missing rule files are ignored. Invalid YAML or invalid rule schema fails the
command before checking files.

## What Check Mode Evaluates

Check mode evaluates provider-independent file rules:

| Rule surface | CI support | Notes |
|--------------|------------|-------|
| `PreToolUse` `Write` with `content` | Yes | The current file body is checked as the would-be write content. |
| `PreToolUse` `Edit` with `new_content` | Yes | The current file body is checked as the would-be edited content. |
| `kind: Regex` content matchers | Yes | Capture groups and message interpolation work. |
| `kind: SyntaxTree` content matchers | Yes | Uses the configured tree-sitter language. |
| `kind: External` content matchers | Yes | The file body is piped to stdin. A non-zero command exit means the rule matched. |
| `target: { kind: Content }` | Yes | Default behavior. Matchers evaluate the raw file body. |
| `target: { kind: MarkdownCodeBlock }` | Yes | Matchers evaluate fenced Markdown code blocks for the configured language. |
| `action: block` | Yes | Violations are printed and the command exits `1`. |
| `action: substitute` | No | Substitutions need a live Bash hook payload and provider `updatedInput`. |
| `PreToolUse` `Bash` | No | Bash rules inspect commands, not files. |
| `PreToolUse` `WebFetch` | No | WebFetch rules inspect URLs and prompts, not files. |
| `UserPromptSubmit` | No | Prompt reminders need a live user prompt. |
| `PermissionRequest` | No | Permission requests are parsed in hook mode but not matchable from YAML. |
| `Delete` | No | Delete events are normalized in hook mode but not matchable from YAML yet. |
| Workflows | No | Workflow gates depend on live hook state. |

Check mode scans files as UTF-8 text. Files that cannot be read as text are
skipped at debug log level, so keep rules targeted with `file` globs.

## File Content Targets

Write/Edit file rules evaluate `target: { kind: Content }` by default. This
checks the raw file body, which is the right target for ordinary source files.

For Markdown files, `target: { kind: MarkdownCodeBlock, language: rust }`
extracts fenced code blocks with a matching info string and evaluates all
content matchers against each block body. A rule matches only when all content
matchers match the same fenced block. Reported file paths and line numbers
still refer to the physical Markdown file:

```yaml
version: 1
rules:
  - name: no-rust-lhs-type-annotations-in-docs
    message: "Use inferred local types in this Rust example."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.md"
        target:
          kind: MarkdownCodeBlock
          language: rust
        content:
          - kind: SyntaxTree
            language: rust
            query: "(let_declaration type: (_) @type)"
```

## Supported Syntax Languages

`kind: SyntaxTree` works in check mode for every supported Nudge language:

- `rust`
- `typescript`
- `javascript`
- `python`
- `go`
- `java`
- `csharp` (`c-sharp` is accepted as an alias)
- `kotlin`
- `haskell`

Use `nudge syntaxtree --language <language> <file-or-source>` when writing a
query. This prints the parser's node names so the query can match the actual
grammar shape for that language.

## CI Examples

GitHub Actions:

```yaml
name: Nudge

on:
  pull_request:
  push:
    branches: [main]

jobs:
  nudge:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Nudge
        run: curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash
      - name: Check Nudge rules
        run: nudge check
```

Local pre-commit hook:

```bash
#!/usr/bin/env bash
set -euo pipefail

nudge check
```

Target only staged paths from another script:

```bash
git diff --cached --name-only --diff-filter=ACMR |
  xargs -r nudge check
```

## Writing CI-Friendly Rules

Rules intended for `nudge check` should use file-based Write or Edit hooks with
clear file globs:

```yaml
version: 1
rules:
  - name: no-unwrap
    description: Require contextual panics in Rust
    message: "Use `.expect(\"...\")` with context instead of `.unwrap()`, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: SyntaxTree
            language: rust
            query: |
              (call_expression
                function: (field_expression
                  field: (field_identifier) @method)
                (#eq? @method "unwrap"))
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: SyntaxTree
            language: rust
            query: |
              (call_expression
                function: (field_expression
                  field: (field_identifier) @method)
                (#eq? @method "unwrap"))
```

For deterministic command rewrites, keep using `action: substitute` in hook
mode. Pair it with a separate file-based block rule only when there is a real
file-state invariant that CI can verify.
