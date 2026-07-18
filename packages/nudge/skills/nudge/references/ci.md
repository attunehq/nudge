# Nudge CI

Use `nudge check` for CI, pre-commit hooks, release gates, and scripts. It runs
Nudge as a one-shot project checker and does not require Claude Code, Codex CLI,
hook installation, trusted project hook layers, or a live agent session.

## Quick Start

```bash
nudge check
nudge check src/ docs/
nudge check src/lib.rs
nudge check "**/*.rs"
```

Exit behavior:

- `0`: no checkable violations were found, or no file-based rules exist.
- `1`: one or more checkable violations were found.
- other non-zero: configuration, argument, or runtime error.

With explicit operands, every path or glob must resolve to at least one file.
Missing paths, empty directories, and glob patterns that match no files fail
before rule evaluation.

## What CI Can Check

`nudge check` evaluates provider-independent file rules:

- `PreToolUse` `Write` rules with `content`
- `PreToolUse` `Edit` rules with `new_content`
- `Regex`, `Contains`, `SyntaxTree`, and `External` content matchers
- `target: { kind: Content }`
- `target: { kind: MarkdownCodeBlock }`
- `action: block`

It does not evaluate live-hook surfaces:

- Bash substitutions
- WebFetch URL rules
- UserPromptSubmit reminders
- PermissionRequest
- Delete policy
- provider-specific workflow gates

If the user wants CI coverage, prefer file-content rules for conventions that
must be enforceable outside a live agent.

## GitHub Actions Example

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

## Local Pre-Commit Example

```bash
#!/usr/bin/env bash
set -euo pipefail

nudge check
```

To check only staged paths from another script:

```bash
git diff --cached --name-only --diff-filter=ACMR |
  xargs -r nudge check
```

## CI-Friendly Rule Shape

Rules intended for CI should use clear file globs and file-content matchers:

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
                arguments: (arguments)
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
                arguments: (arguments)
                (#eq? @method "unwrap"))
```

Use an ordered `file` list to exclude generated or vendored files from one
rule:

```yaml
file:
  - "**/*.rs"
  - "!**/*.gen.rs"
```

The same exclusions apply to live Write and Edit hooks. A list must contain at
least one positive pattern, and the last matching pattern wins.

## Validation

After adding or changing a CI gate:

1. Run `nudge validate`.
2. Run the exact `nudge check ...` command the script or CI job will run.
3. If the rule relies on a live hook surface, explain that `nudge check` cannot
   cover it and add a separate live-agent or hook-level test if needed.
