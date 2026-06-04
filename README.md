# Nudge

Nudge is a **collaborative memory layer** for agent hooks. It remembers the coding conventions, patterns, and preferences that matter to you so Claude Code or Codex CLI can focus on solving your actual problem instead of tracking a mental checklist of stylistic details.

Think of Nudge as a helpful tap on the shoulder: *"Hey, remember this codebase uses turbofish syntax"* rather than a guard checking badges at the door.

**See [docs/PLAN.md](docs/PLAN.md) for project goals and roadmap.**

## The Problem Nudge Solves

Human engineers are (rightfully!) particular about code style. But asking an agent to keep dozens of project-specific preferences in working memory competes with focusing on what you actually want to accomplish.

Nudge offloads those details. You encode your preferences once, and Nudge catches the slips, freeing both you and the agent to think at the level of *"implement this feature"* rather than *"implement this feature, and remember the 47 things I care about."*

## How It Works

Nudge uses agent hook systems to watch supported operations:

- [Claude Code hooks](https://docs.anthropic.com/en/docs/claude-code/hooks)
- [Codex CLI hooks](https://developers.openai.com/codex/hooks)

When something matches a rule you've defined:

- **Interrupt** (PreToolUse rules): Nudge catches the issue *before* it's written and explains what to fix
- **Substitute** (PreToolUse Bash rules): Nudge rewrites simple deterministic commands, lets the tool proceed, and tells the model what changed
- **Continue** (UserPromptSubmit rules): Nudge injects context into the conversation to guide the agent
- **Passthrough**: No rules matched, everything proceeds normally

## Example Rules

These are the rules Nudge uses on its own codebase (yes, we dogfood):

| Preference           | What Nudge reminds agents about                            |
|----------------------|-------------------------------------------------------------|
| No inline imports    | Move `use` statements to the top of the file                |
| LHS type annotations | Prefer turbofish (`::<T>`) over `let x: T = ...`            |
| Qualified paths      | Import and use shorter names instead of long paths          |
| Pretty assertions    | Use `pretty_assertions` in tests for better diff output     |
| No `.unwrap()`       | Use `.expect("...")` with a descriptive message             |

Other Attune codebases of course have other rules.

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

### 2. Install Hooks in Your Project

Navigate to any project where you use Claude Code or Codex CLI and run the setup for the agent you use:

```bash
nudge claude setup
nudge codex setup
```

Claude setup adds Nudge to `.claude/settings.local.json`. Codex setup adds Nudge to `.codex/hooks.json`. You can verify with `/hooks` in the relevant agent.

> [!NOTE]
> Hook configuration is loaded when agent sessions start, so restart open Claude Code or Codex sessions after setup. Future changes to rules are internal to Nudge and therefore do not need an agent restart.

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

Use `nudge check` to validate your entire project against rules, useful for CI pipelines or local linting:

```bash
# Check entire project
nudge check

# Check specific paths or patterns
nudge check src/
nudge check "**/*.rs"

# Use in CI (fails build on violations)
nudge check || exit 1
```

`nudge check` only evaluates file-based block rules for `PreToolUse` Write/Edit matchers. It ignores `action: substitute` rules because substitutions rewrite live Bash hook payloads and need a provider to receive `updatedInput`.

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
