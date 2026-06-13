# Nudge

Nudge is a **collaborative memory layer** for agent hooks. It remembers the coding conventions, patterns, preferences, and hard-won debugging lessons that matter to you so Claude Code or Codex CLI can focus on solving your actual problem instead of tracking a mental checklist.

Think of Nudge as a helpful tap on the shoulder: *"Hey, remember this codebase uses turbofish syntax"* rather than a guard checking badges at the door.

For current rule syntax, run `nudge claude docs` or `nudge codex docs`. Copyable starter rules live in [examples/rules](examples/rules).

## The Problem Nudge Solves

Human engineers are (rightfully!) particular about code style. But asking an agent to keep dozens of project-specific preferences in working memory competes with focusing on what you actually want to accomplish.

Nudge offloads those details. You encode your preferences once, and Nudge catches the slips, freeing both you and the agent to think at the level of *"implement this feature"* rather than *"implement this feature, and remember the 47 things I care about."*

## How It Works

Nudge uses agent hook systems to watch supported operations:

- [Claude Code hooks](https://docs.anthropic.com/en/docs/claude-code/hooks)
- [Codex CLI hooks](https://developers.openai.com/codex/hooks)

For provider-free usage, `nudge check` runs file rules as a one-shot project
checker for CI, pre-commit hooks, and other scripts. See
[CI and programmatic checks](docs/ci.md).

When something matches a rule you've defined:

- **Interrupt** (PreToolUse rules): Nudge catches the issue *before* it's written and explains what to fix
- **Substitute** (PreToolUse Bash rules): Nudge rewrites simple deterministic commands, lets the tool proceed, and tells the model what changed
- **Continue** (UserPromptSubmit rules): Nudge injects context into the conversation to guide the agent
- **Learned context**: Nudge searches `.nudge/learned/*.md` with BM25, or hybrid BM25 plus local embeddings when enabled, and proactively surfaces relevant incident notes
- **Passthrough**: No rules matched, everything proceeds normally

## Example Rules

These are the rules Nudge uses on its own codebase (yes, we dogfood):

| Preference           | What Nudge reminds agents about                            |
|----------------------|-------------------------------------------------------------|
| No inline imports    | Move `use` statements to the top of the file                |
| LHS type annotations | Prefer turbofish (`::<T>`) over `let x: T = ...`            |
| String literals      | Use `String::from("...")` instead of `"...".to_string()`    |
| Qualified paths      | Import and use shorter names instead of long paths          |
| Pretty assertions    | Use `pretty_assertions` in tests for better diff output     |
| No `.unwrap()`       | Use `.expect("...")` with a descriptive message             |

Other Attune codebases of course have other rules.

## Learned Incident Notes

Rules are best for deterministic conventions. Learned notes are for the repo-local "we have seen this before" moments: bugs, failed approaches, root causes, and fixes that future agents should not rediscover from scratch.

Add a note after a debugging session:

```bash
nudge learn add --title "Expo Metro resolver cache" --body "
What went wrong: Expo could not resolve modules after a dependency update.

Fix: clear the Metro cache and restart the dev server.

Verification: expo start completed and the app loaded.
"
```

Or pipe a Markdown note:

```bash
cat incident.md | nudge learn add
```

Notes live in `.nudge/learned/*.md` as plain Markdown. They do not require tags, trigger phrases, or glob metadata. Nudge indexes the title and body dynamically with BM25, which works well for exact error strings, command names, file paths, package names, and stack trace fragments.

Search manually:

```bash
nudge learn search expo metro cannot resolve module
nudge learn list
nudge learn docs
```

During `UserPromptSubmit`, Nudge searches the current prompt against learned notes and injects the top relevant matches as plain context. For supported command surfaces such as Bash and WebFetch, Nudge can also surface learned context as an allow-with-context warning when a tool input resembles a known incident.

Install the bundled `nudge-learnings` skill so agents know how to use the learn command and how to record useful notes:

```bash
nudge claude skills install
nudge codex skills install
```

Claude installs to `.claude/skills/nudge-learnings`. Codex installs to `.agents/skills/nudge-learnings`. The skill uses progressive disclosure: its `SKILL.md` tells the agent to read the BM25 or local-embeddings reference depending on project config.

### Local Embeddings

BM25 is always available and requires no model. For semantic matching across different wording, enable local embeddings in project config:

```bash
nudge learn embeddings enable
nudge learn embeddings status
nudge learn embeddings reindex
```

This writes config like:

```yaml
version: 1
rules: []
learn:
  embeddings:
    enabled: true
    model: BGESmallENV15
```

Nudge also reads `.nudge.yml` if that is your project convention. The compiled Nudge binary includes local embedding support; config decides whether a project uses it.

Model files and vector indexes are stored in the user-level Nudge cache directory selected by the OS through the `directories` crate. They are not stored in the repo because vectors are derived from repo notes, can reveal note content, and change when the model or chunking changes. Learned Markdown notes stay in the repo; generated embedding artifacts stay in user cache.

## Writing Effective Rules

Nudge is a collaborative partner, but **trusted partners can be blunt**.

When an agent is deep in implementation, gentle suggestions get lost in the noise. A soft *"you might want to consider..."* will likely be ignored. A direct *"Stop. Move this import to the top of the file."* gets attention.

This isn't about being harsh, it's about being effective. Think of a rally copilot: they say "HARD LEFT NOW" not because they're angry, but because that's what cuts through when the driver is focused. The trust is what *allows* the directness.

**Guidelines for rule messages:**

- **Be specific**: "Move this import to the top of the file" not "Consider reorganizing imports"
- **Be direct**: "Stop. Fix this first." not "You might want to think about..."
- **Explain why** (briefly): "Use turbofish; LHS annotations clutter the variable name"
- **Give the fix**: Don't just say what's wrong; say what to do instead
- **Use suggestions**: Capture groups let you generate context-aware fixes (see `nudge claude docs` or `nudge codex docs`)
- **End with "then retry"**: Tell the agent to retry the operation after fixing
- **Write for one match**: Your message appears at each match location in a code snippet

Nudge displays violations like Rust compiler errors. Your message appears directly at the matched code:

```
error: rule violation
  |
2 |     use std::io;
  | ^^^^^^^^ Move this import to the top of the file, then retry.
3 |     use std::fs;
  | ^^^^^^^^ Move this import to the top of the file, then retry.
  |
```

The pattern: **what's wrong** -> **how to fix** -> **retry**.

For simple mechanical Bash command rewrites, use `action: substitute` with a regex `replace:` template instead of a blocking message:

```yaml
version: 1
rules:
  - name: use-yarn-add
    action: substitute
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install(?: (?P<args>.*))?$"
            replace: "yarn add {{ $args }}"
```

Substitutions work for Claude Code and Codex CLI. Nudge returns the provider's full updated tool input with only `command` changed, and adds `hookSpecificOutput.additionalContext` so the model sees what was rewritten.

### External Matcher Trust Model

`kind: External` runs a command from your rule YAML with the file content piped to stdin. Treat rule files as trusted local code: do not install or run Nudge rules from a source you would not trust to execute shell commands on your machine.

External commands fail closed. By default, each command has 5000ms to finish; set `timeout_ms` on the matcher when a legitimate checker needs a different bound. If you set `timeout_ms: 0`, Nudge waits indefinitely for that command. A non-zero exit status, missing command, spawn failure, wait failure, or timeout all count as a match so a broken external checker does not silently make a rule inert. Use `{{ $command }}` and `{{ $external_status }}` in the rule message to show the command and what happened.

```yaml
content:
  - kind: External
    command: ["markdownlint", "--stdin"]
    timeout_ms: 10000
```

### Markdown Code Blocks

File rules can choose which part of a matched file is checked with `target`.
The default is `target: { kind: Content }`, which evaluates matchers against
the raw file content. For Markdown files, use `MarkdownCodeBlock` to evaluate
the same Regex, SyntaxTree, or External matchers against fenced code blocks for
a specific language:

```yaml
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

All content matchers in a Markdown code-block target must match the same fenced
block. Snippets and `nudge check` line numbers point back to the physical
Markdown file.

For the full rule syntax and copy-pasteable examples, run `nudge claude docs` or `nudge codex docs`.

### Rule Writing Is Iterative

If an agent ignores a rule, **the fix is usually to make the message more direct**, not to give up on the rule.

Attune dogfoods Nudge on its own codebase and on other codebases we manage. When we notice an agent routing around a rule or missing the point, we tune the message until it lands. Treat ignored rules as feedback on clarity, not evidence that rules don't work.

The collaborative spirit lives in *why* Nudge exists (to help the agent focus on your real problem), not in tiptoeing around feedback.

## Setup

### 1. Install Nudge

**macOS / Linux:**

```bash
curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
```

**From source:**

```bash
git clone https://github.com/attunehq/nudge
cd nudge
cargo install --path packages/nudge
```

### 2. Install Hooks and Skills in Your Project

Navigate to any project where you use Claude Code or Codex CLI and run the setup for the agent you use:

```bash
nudge claude setup
nudge codex setup
```

Claude setup adds Nudge to `.claude/settings.local.json` and installs the bundled learnings skill to `.claude/skills/nudge-learnings`. Codex setup adds Nudge to `.codex/hooks.json` and installs the skill to `.agents/skills/nudge-learnings`. If the target hook file already exists, setup first writes a non-overwriting backup next to it, such as `settings.local.json.bak` or `hooks.json.bak.1`, and prints the backup path. You can verify hooks with `/hooks` in the relevant agent.

> [!NOTE]
> Hook and skill configuration is loaded when agent sessions start, so restart open Claude Code or Codex sessions after setup. Future changes to rules are internal to Nudge and therefore do not need an agent restart.

### 3. Use Your Agent Normally

Nudge runs automatically as you use Claude Code or Codex CLI. No changes to your workflow required.

## Seeing It Work

### In Practice

Write some rules for things that you want to be enforced, and then just use your agent normally. You should see Nudge interject when the rules are violated and help the agent stay on track.

### Debug Mode

Run your agent with debug logging to see hook execution. For Claude Code:

```bash
claude --debug
```

You'll see Nudge's hook being called and its response in the logs.

### CI / Linting Mode

Use `nudge check` to validate your project against file rules without Claude
Code, Codex CLI, or installed hooks. This is the right mode for CI pipelines,
pre-commit checks, release gates, and other programmatic usage.

```bash
# Check entire project
nudge check

# Check specific paths or patterns
nudge check src/
nudge check "**/*.rs"

# Use in CI (fails build on violations)
nudge check || exit 1
```

When you pass explicit paths or globs, each one must resolve to at least one
file. Missing paths, empty directories, and glob patterns that match no files
fail with a non-zero exit so CI scripts do not silently check nothing.

`nudge check` evaluates file-based block rules for `PreToolUse` Write/Edit
matchers, including Regex, SyntaxTree, and External content matchers. It
supports SyntaxTree rules for Rust, TypeScript, JavaScript, Python, Go, Java,
C#, Kotlin, and Haskell. It also supports `target: { kind: MarkdownCodeBlock }`
for fenced code blocks inside Markdown files. Hook-only behavior such as Bash
substitutions, WebFetch, UserPromptSubmit reminders, permissions, delete
events, and workflows still belongs to live hook mode.

Example output when violations are found:

```
x Found 3 issues in 2 files

./src/main.rs:42 [no-unwrap]
  Use `.expect("descriptive error message")` instead of `.unwrap()`, then retry.

./src/lib.rs:15 [no-inline-imports]
  Move this `use` statement to the top of the file, then retry.

./src/lib.rs:23 [no-inline-imports]
  Move this `use` statement to the top of the file, then retry.

Checked 25 files against 6 rules
```

When all checks pass:

```
Checked 25 files against 6 rules
  - .nudge.yaml: 6 rules
```

For the full check-mode contract, CI examples, and supported-feature matrix,
see [CI and programmatic checks](docs/ci.md).

### Manual Testing

You can test a specific rule with the `test` subcommand:

```bash
nudge test --rule no-inline-imports --tool Write --file test.rs \
  --content $'fn main() {\n    use std::io;\n}'
```

Or pipe raw hook JSON to nudge directly:

```bash
echo '{
  "hook_event_name": "PreToolUse",
  "session_id": "test",
  "transcript_path": "/tmp/test",
  "permission_mode": "default",
  "cwd": "/tmp",
  "tool_name": "Write",
  "tool_use_id": "123",
  "tool_input": {
    "file_path": "test.rs",
    "content": "fn main() {\n    use std::io;\n}"
  }
}' | nudge claude hook

# Codex sends apply_patch for file writes and edits. Nudge normalizes supported
# add/update/delete patches into Write/Edit/Delete before evaluating rules, and
# warns the model when an apply_patch input cannot be inspected.
echo '{
  "hook_event_name": "PreToolUse",
  "cwd": "/tmp",
  "tool_name": "apply_patch",
  "tool_input": {
    "command": "*** Begin Patch\n*** Add File: test.rs\n+fn main() {\n+    use std::io;\n+}\n*** End Patch\n"
  }
}' | nudge codex hook

# Exit 0 with JSON output = Interrupt or Substitute (rule matched)
# Exit 0 with plain text = Continue (UserPromptSubmit context injected)
# Exit 0 with no output = Passthrough (nothing to note)
```

## Development

See [CLAUDE.md](CLAUDE.md) for development instructions, architecture overview, and how to add new rules.
