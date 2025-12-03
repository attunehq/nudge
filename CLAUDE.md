# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## The Spirit of Pavlov

Pavlov is a **collaborative partner**, not a rule enforcer. It helps you remember coding conventions so you can focus on the user's actual problem. Internalize these points:

1. **Pavlov is on your side.** When it sends a message, that's a colleague tapping your shoulder—not a reprimand.
2. **Direct ≠ hostile.** Messages are blunt because that's what cuts through when you're focused. Trust the feedback.
3. **Don't route around it.** If you're tempted to work around a Pavlov message, pause. Either follow the rule, or flag that the rule needs fixing.

For the full philosophy (why Pavlov exists, the "collaborative memory layer" framing, the rally copilot analogy), see [README.md](README.md).

### Working on Pavlov Itself

Pavlov is dogfooded here. Your experience using it is direct feedback:

- **Rule feels unclear?** That's signal to improve the wording. Mention it.
- **Rule feels wrong?** Let's fix the rule, not route around it.

## Build and Test Commands

```bash
# Build
cargo build -p pavlov

# Run all tests
cargo test -p pavlov

# Run a specific test
cargo test -p pavlov test_name

# Run the CLI
cargo run -p pavlov -- claude hook      # Respond to hook (reads JSON from stdin)
cargo run -p pavlov -- claude setup     # Install hooks into .claude/settings.json
```

## Architecture

### CLI Structure

```
pavlov claude hook   - Receives hook JSON on stdin, evaluates rules, outputs response
pavlov claude setup  - Writes hook configuration to .claude/settings.json
```

### Module Layout

- `src/main.rs` - CLI entry point using clap
- `src/cmd/claude/hook.rs` - Hook command: deserializes input, calls `rules::evaluate_all()`, emits response
- `src/cmd/claude/setup.rs` - Setup command: configures hooks in settings.json
- `src/rules.rs` - All rule functions and `evaluate_all()` dispatcher
- `src/claude/hook.rs` - Types for hook payloads and responses (Hook, Response, etc.)

### How Pavlov Communicates

When Pavlov has something to share, it responds in one of three ways:

- **Passthrough**: Nothing to note—carry on!
- **Continue**: The code is written, and Pavlov sends you a gentle reminder to consider
- **Interrupt**: Pavlov caught something worth fixing first—it'll explain what and why

## Keeping Documentation in Sync

Pavlov has three documentation sources that must stay aligned. When updating one, consider whether the others need updates too.

| Document | Audience | Purpose | Focus |
|----------|----------|---------|-------|
| **CLAUDE.md** | You, developing Pavlov | How Pavlov works under the hood | Architecture, internals, testing patterns |
| **README.md** | Humans evaluating or contributing | Why Pavlov exists and what it believes | Philosophy, motivation, the collaborative framing |
| **`pavlov claude docs`** | You or humans writing rules elsewhere | How to write rules (reference card) | Rule syntax, template variables, examples |

**CLAUDE.md** (this file) is for *developing* Pavlov—understanding the module layout, how to add features, how tests work.

**README.md** is for *understanding* Pavlov—the philosophy that Pavlov is a collaborative partner, why directness matters, how to write effective rules. This is the front door; it needs to convey the spirit.

**`pavlov claude docs`** (`src/cmd/claude/docs.rs`) is for *using* Pavlov—a self-contained reference that future Claude instances or humans can consult when writing rules. It should be scannable, copy-pasteable, and not assume any prior context.

When you change something fundamental (like adding a template variable, changing the rule format, or refining the collaborative framing), update all three.
