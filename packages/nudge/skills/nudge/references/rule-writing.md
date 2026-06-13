# Nudge Rule Writing

## Rule Locations

Nudge loads all matching config files additively:

- user-level `rules.yaml`
- project `.nudge.yaml`
- project `.nudge.yml`
- project `.nudge/**/*.{yaml,yml}`

Use project files for repo conventions. Use user-level rules for personal
preferences that should follow the user across repositories.

## Commands

```bash
nudge validate
nudge check
nudge check src/ docs/
nudge test
nudge claude docs
nudge codex docs
```

`nudge claude docs` and `nudge codex docs` print the full copy-pasteable rule
reference. This file is the in-skill quick reference.

## Rule Shape

```yaml
version: 1

rules:
  - name: rule-identifier
    description: Human-readable description
    action: block
    message: "Tell the agent what is wrong and how to fix it."
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: Regex
            pattern: "\\bunwrap\\("
            suggestion: "Return or handle the error instead of unwrapping."
```

Important defaults:

- `action` defaults to `block`.
- `message` should be actionable and direct.
- `on` is a list; any matcher can trigger the rule.
- `target` defaults to raw file content with `kind: Content`.

## Hook And Tool Matchers

Use `PreToolUse` when matching an attempted operation:

- `tool: Write` for new file content
- `tool: Edit` for edit replacement content
- `tool: WebFetch` for URL fetches
- `tool: Bash` for shell commands

Use `UserPromptSubmit` when injecting guidance at turn start.

## Content Targets

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

Line numbers and snippets still point back to the physical Markdown file.

## Pattern Kinds

Common content matcher kinds:

- `Regex`: regular expression match with optional `replace` and `suggestion`
- `Contains`: exact substring match
- `TreeSitter`: syntax-aware query for supported languages

Use syntax-aware rules when regex would produce noisy matches in comments,
strings, or unrelated syntax.

## Bash Substitute Example

```yaml
version: 1

rules:
  - name: use-yarn-add
    description: Use yarn add instead of npm install
    action: substitute
    message: "Use yarn add for this project."
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install(?: (?P<args>.*))?$"
            replace: "yarn add ${args}"
```

Substitution is for deterministic rewrites. Do not use it when judgment or
interactive confirmation is needed.

## Message Guidelines

Good messages answer:

1. What matched?
2. Why does it matter?
3. What should the agent do next?

Prefer direct wording:

```yaml
message: "Do not use unwrap in Rust docs examples. Show explicit error handling or explain why panicking is required."
```

Avoid vague wording:

```yaml
message: "Please follow best practices."
```

## Validation Workflow

After editing rules, read [validation.md](validation.md) and run the checks that
prove the changed rule parses and behaves as intended.
