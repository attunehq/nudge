# Pavlov

Pavlov is a **collaborative memory layer** for Claude Code. It remembers the coding conventions, patterns, and preferences that matter to you—so Claude can focus on solving your actual problem instead of tracking a mental checklist of stylistic details.

Think of Pavlov as a helpful tap on the shoulder: *"Hey, remember this codebase uses turbofish syntax"* rather than a guard checking badges at the door.

**See [docs/PLAN.md](docs/PLAN.md) for project goals and roadmap.**

## The Problem Pavlov Solves

Human engineers are (rightfully!) particular about code style. But asking Claude to keep dozens of project-specific preferences in working memory competes with focusing on what you actually want to accomplish.

Pavlov offloads those details. You encode your preferences once, and Pavlov catches the slips—freeing both you and Claude to think at the level of *"implement this feature"* rather than *"implement this feature, and remember the 47 things I care about."*

## How It Works

Pavlov uses Claude Code's [hooks system](https://docs.anthropic.com/en/docs/claude-code/hooks) to watch `Write` and `Edit` operations. When something matches a rule you've defined:

- **Continue**: The code is written, and Pavlov injects a gentle reminder for Claude to consider
- **Interrupt**: Pavlov catches the issue before it's written and explains what to do instead
- **Passthrough**: No opinion, everything proceeds normally

## Example Rules

These are the rules Pavlov uses on its own codebase (yes, we dogfood). All use **Continue**—gentle reminders rather than hard stops:

| Preference           | What Pavlov reminds Claude about                            |
|----------------------|-------------------------------------------------------------|
| No inline imports    | Move `use` statements to the top of the file                |
| Field spacing        | Add blank lines between struct fields for readability       |
| LHS type annotations | Prefer turbofish (`::<T>`) over `let x: T = ...`            |
| Qualified paths      | Import and use shorter names instead of long paths          |
| Pretty assertions    | Use `pretty_assertions` in tests for better diff output     |

## Writing Effective Rules

Pavlov is a collaborative partner, but **trusted partners can be blunt**.

When Claude is deep in implementation, gentle suggestions get lost in the noise. A mealy-mouthed *"you might want to consider..."* will be ignored. A direct *"Stop. Move this import to the top of the file."* gets attention.

This isn't about being harsh—it's about being effective. Think of a rally copilot: they say "HARD LEFT NOW" not because they're angry, but because that's what cuts through when the driver is focused. The trust is what *allows* the directness.

**Guidelines for rule messages:**

- **Be specific**: "Move `use` statements to the top of the file" not "Consider reorganizing imports"
- **Be direct**: "Stop. Fix this first." not "You might want to think about..."
- **Explain why** (briefly): "Use turbofish—LHS annotations clutter the variable name"
- **Give the fix**: Don't just say what's wrong; say what to do instead
- **End with "then retry"**: Tell Claude to retry the operation after fixing
- **Use template variables**: `{{ lines }}`, `{{ file_path }}`, etc. to point to exactly what needs to change

The pattern: **what's wrong** → **where** → **how to fix** → **retry**.

For the full rule syntax, template variables, and copy-pasteable examples, run `pavlov claude docs`.

### Rule Writing Is Iterative

If Claude ignores a rule, **the fix is usually to make the message more direct**—not to give up on the rule.

Pavlov is dogfooded on its own codebase. When we notice Claude routing around a rule or missing the point, we tune the message until it lands. Treat ignored rules as feedback on clarity, not evidence that rules don't work.

The collaborative spirit lives in *why* Pavlov exists (to help Claude focus on your real problem), not in tiptoeing around feedback.

## Setup

### 1. Build Pavlov

```bash
git clone https://github.com/attunehq/pavlov
cd pavlov
cargo install --path packages/pavlov
```

### 2. Install Hooks in Your Project

Navigate to any project where you use Claude Code and run:

```bash
pavlov claude setup
```

This adds Pavlov to `.claude/settings.json`. You can verify with `/hooks` in Claude Code.

### 3. Use Claude Code Normally

Pavlov runs automatically when Claude tries to write or edit files. No changes to your workflow required.

## Seeing It Work

### Quick Test

> [!TIP]
> If your user settings conflict with Pavlov's rules, you may need to
> temporarily rename `~/.claude/CLAUDE.md`.

Ask Claude to write some code and watch Pavlov help out:

**Try asking for code that might slip on a rule:**
> "Write a Rust function that uses a HashMap"

If Claude writes `use std::collections::HashMap` inside the function body, Pavlov will gently remind it to move imports to the top. Claude sees the reminder and can adjust—no friction, no frustration.

### Debug Mode

Run Claude Code with debug logging to see hook execution:

```bash
claude --debug
```

You'll see Pavlov's hook being called and its response in the logs.

### Manual Testing

You can test Pavlov directly by piping hook JSON to it:

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
}' | cargo run -p pavlov -- claude hook

# Exit code 2 = Interrupt (Pavlov caught something, asks Claude to reconsider)
# Exit code 0 with JSON = Continue (code written, with a helpful reminder)
# Exit code 0 with no output = Passthrough (nothing to note)
echo $?
```

## Development

See [CLAUDE.md](CLAUDE.md) for development instructions, architecture overview, and how to add new rules.

```bash
# Run tests
cargo test -p pavlov

# Build
cargo build -p pavlov
```
