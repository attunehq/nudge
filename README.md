# Pavlov

Pavlov adds "teachability" to Claude Code by intercepting tool calls and enforcing coding rules before code is written. When Claude tries to write code that violates a rule, Pavlov either blocks the operation (for hard rules) or injects guidance into the conversation (for soft suggestions).

**See [PLAN.md](PLAN.md) for project goals and roadmap.**

## How It Works

Pavlov uses Claude Code's [hooks system](https://docs.anthropic.com/en/docs/claude-code/hooks) to intercept `Write` and `Edit` tool calls. Each hook event is evaluated against a set of rules:

- **Interrupt**: Blocks the tool call and tells Claude to fix the issue first
- **Continue**: Allows the tool call but injects guidance for Claude to consider
- **Passthrough**: No opinion, tool proceeds normally

## Current Rules

All current rules use **Continue** responses: the code is written, but guidance is injected into the conversation for Claude to consider. **Interrupt** is available for rules that should block code from being written.

| Rule | Trigger |
|------|---------|
| No inline imports | `use` statements inside function bodies |
| Field spacing | Consecutive struct fields/enum variants without blank lines |
| LHS type annotations | `let foo: Type = ...` instead of turbofish |
| Qualified paths | Over-qualified paths like `foo::bar::baz::func()` |
| Pretty assertions | `assert_eq!` in tests without `pretty_assertions` |

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

Ask Claude to write code that triggers a rule. Try one of these prompts:

**Trigger `no_inline_imports`:**
> "Write a Rust function that imports `std::collections::HashMap` inside the function body"

**Trigger `require_field_spacing`:**
> "Create a Rust struct called `Config` with fields `name: String` and `port: u16` on consecutive lines"

**Trigger `no_lhs_type_annotations`:**
> "Write a Rust function that declares `let items: Vec<String> = vec![]`"

**Trigger `prefer_pretty_assertions`:**
> "Write a Rust test that uses `assert_eq!`"

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

# Exit code 2 = Interrupt (blocked)
# Exit code 0 with JSON = Continue (allowed with guidance)
# Exit code 0 with no output = Passthrough
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
