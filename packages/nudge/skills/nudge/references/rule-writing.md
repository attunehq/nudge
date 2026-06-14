# Nudge Rule Writing

Use this reference when writing or changing `.nudge.yaml`, `.nudge.yml`, or
`.nudge/**/*.yaml`. Rules are deterministic conventions. Learned incident notes
are repo memory; use [learnings.md](learnings.md) for those.

## Rule Locations

Nudge loads all matching config files additively:

- user-level `rules.yaml`
- project `.nudge.yaml`
- project `.nudge.yml`
- project `.nudge/**/*.{yaml,yml}`

User-level `rules.yaml` lives under the platform config directory:

- Linux: `~/.config/nudge/rules.yaml`
- macOS: `~/Library/Application Support/com.attunehq.nudge/rules.yaml`
- Windows: `%APPDATA%\attunehq\nudge\config\rules.yaml`

Use project files for repo conventions. Use user-level rules for personal
preferences that should follow the user across repositories.

## Commands

```bash
nudge validate
nudge test
nudge test --help
nudge check
nudge check src/ docs/
nudge syntaxtree --language rust src/lib.rs
```

Read [validation.md](validation.md) before choosing proof commands. Read
[ci.md](ci.md) for scripted gates.

## Rule Shape

```yaml
version: 1

rules:
  - name: rule-identifier
    description: Human-readable description
    action: block
    message: "Tell the agent what is wrong and how to fix it, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        target:
          kind: Content
        content:
          - kind: Regex
            pattern: "\\bunwrap\\("
            suggestion: "Return or handle the error instead of unwrapping."

      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: Regex
            pattern: "\\bunwrap\\("
```

Important defaults:

- `action` defaults to `block`. The other supported action is `substitute`.
- `description` is optional.
- `message` is required for useful block rules and should be actionable.
- `on` is a list; any matcher can trigger the rule.
- `target` defaults to raw file content with `kind: Content`.

## Hooks And Tools

Use `PreToolUse` when matching an attempted operation:

- `tool: Write`: match content being written to a new file with `content:`
- `tool: Edit`: match replacement text with `new_content:`
- `tool: WebFetch`: match fetched URLs with `url:`
- `tool: Bash`: match shell commands with `command:`

Use `UserPromptSubmit` when injecting guidance at turn start:

```yaml
version: 1
rules:
  - name: dev-server-hint
    message: "Use the repo's existing dev-server command before inventing a new one."
    on:
      - hook: UserPromptSubmit
        prompt:
          - kind: Regex
            pattern: "(?i)start.*(server|dev)|run.*local"
```

`UserPromptSubmit` rules always continue by injecting context. `PreToolUse`
block rules interrupt supported operations. `PreToolUse` Bash substitute rules
rewrite deterministic commands and allow the updated command to run.

## Provider Support

Read [hook-responses.md](hook-responses.md) when a rule behaves differently
between providers.

Supported YAML surfaces today:

- Claude Code: `Write`, `Edit`, `WebFetch`, `Bash`, and `UserPromptSubmit`
- Codex CLI: `UserPromptSubmit`, `Write` through `apply_patch` add-file parsing,
  `Edit` through `apply_patch` update parsing, and partial Bash coverage

`Delete` and `PermissionRequest` are normalized internally, but do not have YAML
matchers yet. Write Codex file rules in terms of `Write` and `Edit`; `apply_patch`
is an adapter detail.

## File Content Targets

File rules evaluate `target:` before running `content:` or `new_content:`.

Raw content is the default:

```yaml
target:
  kind: Content
```

Markdown code blocks can be targeted without matching prose:

```yaml
target:
  kind: MarkdownCodeBlock
  language: rust
```

`MarkdownCodeBlock` evaluates fenced code blocks for one language. The language
comes from the first code-fence info-string word, such as `rust`, `rs`, `ts`, or
`python`. Snippets and `nudge check` line numbers point back to the physical
Markdown file. All content matchers must match the same target string.

## Matcher Kinds

Use these matcher kinds in `content:`, `new_content:`, `command:`, `prompt:`,
and supported project-state fields:

- `Regex`: regular expression match with optional `replace` and `suggestion`
- `SyntaxTree`: tree-sitter query for supported languages
- `External`: run a trusted local command against file content

Use `Regex` for simple text patterns, `SyntaxTree` for structure, and `External`
when an existing checker is the clearest source of truth.

Use `Regex` under `url:`. URL matchers support `pattern:` and optional
`suggestion:`.

## Regex

All regex patterns use Rust regex syntax. Add inline flags at the start for
modifiers, and combine them like `(?im)`.

- `(?i)`: case-insensitive
- `(?m)`: multiline mode; `^` and `$` match line boundaries
- `(?s)`: dot matches newline
- `(?R)`: CRLF mode when multiline is enabled
- `(?U)`: ungreedy mode
- `(?u)`: Unicode mode, enabled by default
- `(?x)`: verbose mode with ignored whitespace and `#` comments

Example:

```yaml
- kind: Regex
  pattern: "(?m)^[ \\t]+use "
```

## Suggestions And Captures

Use `suggestion:` to make messages context-aware:

```yaml
version: 1
rules:
  - name: no-unwrap
    description: Use expect with context instead of unwrap
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: Regex
            pattern: "(?P<expr>\\w+)\\.unwrap\\(\\)"
            suggestion: "Replace {{ $expr }}.unwrap() with {{ $expr }}.expect(\"...\")"
```

Capture syntax:

- `{{ $1 }}` and `{{ $2 }}` reference positional captures.
- `{{ $name }}` references named captures from `(?P<name>...)`.
- `{{ $suggestion }}` inserts the interpolated matcher suggestion into the
  rule message.

Interpolation is two phase: the matcher suggestion sees captures first, then the
rule message can reference `{{ $suggestion }}`. Each match gets its own
capture-specific suggestion.

## SyntaxTree

Use `kind: SyntaxTree` for AST-aware rules:

```yaml
content:
  - kind: SyntaxTree
    language: typescript
    query: |
      (function_declaration
        name: (identifier) @fn_name)
    suggestion: "Review function {{ $fn_name }}."
```

Supported languages:

- `rust`
- `typescript`
- `javascript`
- `python`
- `go`
- `java`
- `csharp`
- `kotlin`
- `haskell`

Tree-sitter queries use S-expressions. Captures use `@name` and can be
referenced in suggestions. Use `nudge syntaxtree --language <language>
<file-or-source>` to inspect node names while debugging.

Use `SyntaxTree` when regex would match comments, strings, or unrelated syntax.
If a parser cannot produce a tree, the matcher passes silently. Tree-sitter
usually recovers from incomplete syntax, so useful matches can still fire while
an agent is mid-edit.

## External

Use `kind: External` to delegate file-content matching to a trusted local
program:

```yaml
content:
  - kind: External
    command: ["npx", "markdownlint", "--stdin"]
    timeout_ms: 10000
```

Nudge pipes the candidate content to the command's stdin:

- exit code `0`: no match
- non-zero exit: match
- missing command, spawn/wait failure, or timeout: match

External matchers fail closed so missing checkers do not silently disable
policy. They expose `{{ $command }}` and `{{ $external_status }}` captures for
messages:

```yaml
message: |
  Format this Markdown table so columns are aligned.
  Status: {{ $external_status }}
  Pipe the content to `{{ $command }}` to inspect the checker output.
```

Rule YAML with `External` matchers is trusted local code. Do not run rules from
a source you would not trust to execute commands on your machine. `timeout_ms`
defaults to `5000`; `0` waits indefinitely.

External matchers do not identify specific spans, so they do not render precise
code snippets.

## Bash Project State

`Bash` rules can add `project_state:` matchers. All project-state matchers must
pass before the command matchers run.

```yaml
version: 1
rules:
  - name: block-main-push
    description: Block git push on main branch
    message: "`git push` is not allowed on `main`. Create a feature branch first."
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "git\\s+push"
        project_state:
          - kind: Git
            branch:
              - kind: Regex
                pattern: "^main$"
```

Available project state:

- `kind: Git` with `branch:` matchers against the current branch name

If the current directory is not in a Git repo, `Git` project-state matchers log
a warning and return false.

## Substitution Rules

Use `action: substitute` for deterministic Bash command rewrites:

```yaml
version: 1
rules:
  - name: use-yarn-add
    description: Use yarn add instead of npm install
    action: substitute
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install(?: (?P<args>.*))?$"
            replace: "yarn add {{ $args }}"
```

Substitution rules apply regex `replace:` templates in rule order, return the
provider's full updated tool input, and add provider-specific context so the
model can see what changed. Use substitution only for mechanical rewrites. Do
not use it when judgment or interactive confirmation is needed.

`nudge check` ignores substitute rules because check mode scans repository files
against file-based block rules. Substitutions need a live Bash hook payload and
a provider that can receive updated input.

## Messages

When a block rule matches file content, Nudge displays a compiler-style snippet
and prints your message at each match location. Write the message for one
occurrence.

Good messages answer:

1. What matched?
2. Why does it matter?
3. What should the agent do next?

Prefer direct wording:

```yaml
message: "Move this import to the top of the file, then retry."
```

Avoid vague wording:

```yaml
message: "Please follow best practices."
```

If an agent ignores a rule, make the message clearer and more actionable. Treat
ignored rules as feedback on clarity.

## Examples

### Block Indented Imports

```yaml
version: 1
rules:
  - name: no-inline-imports
    description: Move imports to the top of the file
    message: "Move this import to the top of the file, then retry."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: Regex
            pattern: "(?m)^[ \\t]+use "
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: Regex
            pattern: "(?m)^[ \\t]+use "
```

### Block Rust Syntax In Markdown Code Blocks

```yaml
version: 1
rules:
  - name: no-rust-lhs-type-annotations-in-docs
    description: Use inferred local types in Rust Markdown examples
    message: "Use type inference in this Rust code block, then retry."
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

### Redirect WebFetch To Local Source

```yaml
version: 1
rules:
  - name: prefer-local-docs
    description: Read local crate source instead of fetching from docs.rs
    message: "{{ $suggestion }}"
    on:
      - hook: PreToolUse
        tool: WebFetch
        url:
          - kind: Regex
            pattern: "docs\\.rs/(?P<crate>[^/]+)"
            suggestion: "Read local source at ~/.cargo/registry/src/*/{{ $crate }}-*"
```

### Run An External Markdown Checker

```yaml
version: 1
rules:
  - name: format-markdown-tables
    description: Ensure Markdown tables have aligned columns
    message: |
      Format this Markdown table so columns are aligned.
      Status: {{ $external_status }}
      Pipe the content to `{{ $command }}` to inspect the checker output.
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.md"
        content:
          - kind: External
            command: ["npx", "markdownlint", "--stdin"]
            timeout_ms: 10000
```

### Block Dangerous Shell Commands

```yaml
version: 1
rules:
  - name: block-dangerous-rm
    description: Block dangerous rm commands
    message: "This command could delete critical files. Verify the path first."
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "rm\\s+-[rf]*\\s+/"
```

## Validation Workflow

After editing rules, read [validation.md](validation.md) and run the checks that
prove the changed rule parses and behaves as intended.
