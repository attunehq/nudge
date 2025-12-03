# Nudge

Nudge is a **collaborative memory layer** for Claude Code. It remembers the coding conventions, patterns, and preferences that matter to you- so Claude can focus on solving your actual problem instead of tracking a mental checklist of stylistic details.

Think of Nudge as a helpful tap on the shoulder: *"Hey, remember this codebase uses turbofish syntax"* rather than a guard checking badges at the door.

**See [docs/PLAN.md](docs/PLAN.md) for project goals and roadmap.**

## The Problem Nudge Solves

Human engineers are (rightfully!) particular about code style. But asking Claude to keep dozens of project-specific preferences in working memory competes with focusing on what you actually want to accomplish.

Nudge offloads those details. You encode your preferences once, and Nudge catches the slips- freeing both you and Claude to think at the level of *"implement this feature"* rather than *"implement this feature, and remember the 47 things I care about."*

## How It Works

Nudge uses Claude Code's [hooks system](https://docs.anthropic.com/en/docs/claude-code/hooks) to watch what it does. When something matches a rule you've defined:

- **Continue**: The code is written, and Nudge injects a gentle reminder for Claude to consider
- **Interrupt**: Nudge catches the issue before it's written and explains what to do instead
- **Passthrough**: No opinion, everything proceeds normally

## Example Rules

These are the rules Nudge uses on its own codebase (yes, we dogfood):

| Preference           | What Nudge reminds Claude about                            |
|----------------------|-------------------------------------------------------------|
| No inline imports    | Move `use` statements to the top of the file                |
| Field spacing        | Add blank lines between struct fields for readability       |
| LHS type annotations | Prefer turbofish (`::<T>`) over `let x: T = ...`            |
| Qualified paths      | Import and use shorter names instead of long paths          |
| Pretty assertions    | Use `pretty_assertions` in tests for better diff output     |

Other Attune codebases of course have other rules.

## Writing Effective Rules

Nudge is a collaborative partner, but **trusted partners can be blunt**.

When Claude is deep in implementation, gentle suggestions get lost in the noise. A soft *"you might want to consider..."* will likely be ignored. A direct *"Stop. Move this import to the top of the file."* gets attention.

This isn't about being harsh, it's about being effective. Think of a rally copilot: they say "HARD LEFT NOW" not because they're angry, but because that's what cuts through when the driver is focused. The trust is what *allows* the directness.

**Guidelines for rule messages:**

- **Be specific**: "Move `use` statements to the top of the file" not "Consider reorganizing imports"
- **Be direct**: "Stop. Fix this first." not "You might want to think about..."
- **Explain why** (briefly): "Use turbofish- LHS annotations clutter the variable name"
- **Give the fix**: Don't just say what's wrong; say what to do instead
- **End with "then retry"**: Tell Claude to retry the operation after fixing
- **Use template variables**: `{{ lines }}`, `{{ file_path }}`, etc. to point to exactly what needs to change

The pattern: **what's wrong** → **where** → **how to fix** → **retry**.

For the full rule syntax, template variables, and copy-pasteable examples, run `nudge claude docs`.

### Rule Writing Is Iterative

If Claude ignores a rule, **the fix is usually to make the message more direct**, not to give up on the rule.

Attune dogfoods Nudge on its own codebase and on other codebases we manage. When we notice Claude routing around a rule or missing the point, we tune the message until it lands. Treat ignored rules as feedback on clarity, not evidence that rules don't work.

The collaborative spirit lives in *why* Nudge exists (to help Claude focus on your real problem), not in tiptoeing around feedback.

## Setup

### 1. Build Nudge

```bash
git clone https://github.com/attunehq/nudge
cd nudge
cargo install --path packages/nudge
```

### 2. Install Hooks in Your Project

Navigate to any project where you use Claude Code and run:

```bash
nudge claude setup
```

This adds Nudge to `.claude/settings.json`. You can verify with `/hooks` in Claude Code.

> [!NOTE]
> Claude Code loads hooks on startup, so you'll need to restart open sessions (`claude -c` is an easy way to do this without breaking your flow). Future changes to rules are internal to Nudge and therefore do not need a Claude Code restart.

### 3. Use Claude Code Normally

Nudge runs automatically as you use Claude Code. No changes to your workflow required.

## Seeing It Work

### In Practice

Write some rules for things that you want to be enforced, and then just use Claude Code normally. You should see Nudge interject when the rules are violated and help Claude stay on track.

### Debug Mode

Run Claude Code with debug logging to see hook execution:

```bash
claude --debug
```

You'll see Nudge's hook being called and its response in the logs.

### Manual Testing

You can test Nudge directly by piping hook JSON to it:

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
}' | cargo run -p nudge -- claude hook

# Exit code 2 = Interrupt (Nudge caught something, instructs Claude to reconsider)
# Exit code 0 with JSON = Continue (code written, with a soft reminder)
# Exit code 0 with no output = Passthrough (nothing to note)
echo $?
```

## Development

See [CLAUDE.md](CLAUDE.md) for development instructions, architecture overview, and how to add new rules.
