# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## The Spirit of Nudge

Nudge is a **collaborative partner**, not a rule enforcer. It helps you remember coding conventions so you can focus on the user's actual problem. Internalize these points:

1. **Nudge is on your side.** When it sends a message, that's a colleague tapping your shoulder—not a reprimand.
2. **Direct ≠ hostile.** Messages are blunt because that's what cuts through when you're focused. Trust the feedback.
3. **Don't route around it.** If you're tempted to work around a Nudge message, pause. Either follow the rule, or flag that the rule needs fixing.

For the full philosophy (why Nudge exists, the "collaborative memory layer" framing, the rally copilot analogy), see [README.md](README.md).

### Working on Nudge Itself

Nudge is dogfooded here. Your experience using it is direct feedback:

- **Rule feels unclear?** That's signal to improve the wording. Mention it.
- **Rule feels wrong?** Let's fix the rule, not route around it.

## Build and Test Commands

```bash
# Build
cargo build -p nudge

# Run all tests
cargo test -p nudge

# Run a specific test
cargo test -p nudge test_name

# Run the CLI
cargo run -p nudge -- claude hook      # Respond to hook (reads JSON from stdin)
cargo run -p nudge -- claude setup     # Install hooks into .claude/settings.local.json
cargo run -p nudge -- claude docs      # Print rule writing documentation
cargo run -p nudge -- test             # Test a rule against sample input
cargo run -p nudge -- validate         # Validate rule config files
cargo run -p nudge -- check            # Check project files against rules (for CI)
```

## Architecture

### CLI Structure

```
nudge claude hook   - Receives hook JSON on stdin, evaluates rules, outputs response
nudge claude setup  - Writes hook configuration to .claude/settings.local.json
nudge claude docs   - Prints documentation for writing rules
nudge test          - Test a specific rule against sample input
nudge validate      - Validate and display parsed rule configs
nudge check         - Check project files against rules (CI/linter mode)
```

### Module Layout

- `src/main.rs` - CLI entry point using clap
- `src/cmd/claude/hook.rs` - Hook command: deserializes input, evaluates rules, emits response
- `src/cmd/claude/setup.rs` - Setup command: configures hooks in settings.local.json
- `src/cmd/claude/docs.rs` - Docs command: prints rule writing guide
- `src/cmd/test.rs` - Test command: test a rule against sample input
- `src/cmd/validate.rs` - Validate command: parse and display rule configs
- `src/cmd/check.rs` - Check command: validate project files against rules for CI
- `src/rules.rs` - Rule loading from config files
- `src/rules/schema.rs` - Rule schema types and matchers (serde types that double as evaluators)
- `src/claude/hook.rs` - Hook payload and response types
- `src/snippet.rs` - Code snippet rendering for rule violations (uses `annotate-snippets`)

### How Nudge Communicates

When Nudge has something to share, it responds in one of three ways:

- **Passthrough**: Nothing to note—carry on!
- **Continue**: For UserPromptSubmit hooks, Nudge injects context as plain text
- **Interrupt**: For PreToolUse hooks, Nudge blocks the operation and explains what to fix

The response type is determined by the hook type:
- `PreToolUse` rules always **interrupt** (block the Write/Edit operation)
- `UserPromptSubmit` rules always **continue** (inject guidance into the conversation)

## Keeping Documentation in Sync

Nudge has three documentation sources that must stay aligned. When updating one, consider whether the others need updates too.

| Document | Audience | Purpose | Focus |
|----------|----------|---------|-------|
| **CLAUDE.md** | You, developing Nudge | How Nudge works under the hood | Architecture, internals, testing patterns |
| **README.md** | Humans evaluating or contributing | Why Nudge exists and what it believes | Philosophy, motivation, the collaborative framing |
| **`nudge claude docs`** | You or humans writing rules elsewhere | How to write rules (reference card) | Rule syntax, examples |

**CLAUDE.md** (this file) is for *developing* Nudge—understanding the module layout, how to add features, how tests work.

**README.md** is for *understanding* Nudge—the philosophy that Nudge is a collaborative partner, why directness matters, how to write effective rules. This is the front door; it needs to convey the spirit.

**`nudge claude docs`** (`src/cmd/claude/docs.rs`) is for *using* Nudge—a self-contained reference that future Claude instances or humans can consult when writing rules. It should be scannable, copy-pasteable, and not assume any prior context.

When you change something fundamental (like changing the rule format or refining the collaborative framing), update all three.
