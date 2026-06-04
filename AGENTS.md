# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## The Spirit of Nudge

Nudge is a **collaborative partner**, not a rule enforcer. It helps you remember coding conventions so you can focus on the user's actual problem. Internalize these points:

1. **Nudge is on your side.** When it sends a message, that's a colleague tapping your shoulder, not a reprimand.
2. **Direct does not mean hostile.** Messages are blunt because that's what cuts through when you're focused. Trust the feedback.
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
cargo run -p nudge -- claude hook      # Respond to Claude hook (reads JSON from stdin)
cargo run -p nudge -- claude setup     # Install hooks into .claude/settings.local.json
cargo run -p nudge -- claude docs      # Print rule writing documentation
cargo run -p nudge -- codex hook       # Respond to Codex hook (reads JSON from stdin)
cargo run -p nudge -- codex setup      # Install hooks into .codex/hooks.json
cargo run -p nudge -- codex docs       # Print rule writing documentation
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
nudge codex hook    - Receives hook JSON on stdin, evaluates rules, outputs response
nudge codex setup   - Writes hook configuration to .codex/hooks.json
nudge codex docs    - Prints documentation for writing rules
nudge test          - Test a specific rule against sample input
nudge validate      - Validate and display parsed rule configs
nudge check         - Check project files against rules (CI/linter mode)
```

### Module Layout

- `src/main.rs` - CLI entry point using clap
- `src/agent.rs` - Provider adapters for Claude Code and Codex CLI
- `src/hook.rs` - Normalized hook event model
- `src/hook/evaluate.rs` - Provider-neutral rule evaluation
- `src/hook/response.rs` - Provider-specific response rendering
- `src/hook/apply_patch.rs` - Codex apply_patch normalization
- `src/cmd/claude/hook.rs` - Hook command: deserializes input, evaluates rules, emits response
- `src/cmd/claude/setup.rs` - Setup command: configures hooks in settings.local.json
- `src/cmd/claude/docs.rs` - Docs command: prints rule writing guide
- `src/cmd/codex/hook.rs` - Hook command: deserializes input, evaluates rules, emits response
- `src/cmd/codex/setup.rs` - Setup command: configures hooks in hooks.json
- `src/cmd/codex/docs.rs` - Docs command: prints rule writing guide
- `src/cmd/test.rs` - Test command: test a rule against sample input
- `src/cmd/validate.rs` - Validate command: parse and display rule configs
- `src/cmd/check.rs` - Check command: validate project files against rules for CI
- `src/rules.rs` - Rule loading from config files
- `src/rules/schema.rs` - Rule schema facade and hook matcher types
- `src/rules/schema/` - Focused matcher implementations for content, glob paths, project state, tree-sitter syntax, and URLs
- `src/snippet.rs` - Code snippet rendering for rule violations (uses `annotate-snippets`)

### How Nudge Communicates

When Nudge has something to share, it responds in one of three ways:

- **Passthrough**: Nothing to note. Carry on!
- **Continue**: For UserPromptSubmit hooks, Nudge injects context as plain text
- **Interrupt**: For PreToolUse hooks, Nudge blocks the operation and explains what to fix
- **Warning**: For provider inputs that look like supported PreToolUse surfaces but cannot be inspected (currently Codex apply_patch parse failures), Nudge allows the operation and tells the model to report the warning to the user
- **Substitute**: For deterministic PreToolUse Bash rules, Nudge rewrites the command and lets it proceed

The response type is determined by the hook type:
- `PreToolUse` block rules **interrupt** (block provider-supported Write/Edit/WebFetch/Bash operations)
- `PreToolUse` substitute rules **allow with updated input** (Claude Code and Codex CLI Bash commands)
- `UserPromptSubmit` rules always **continue** (inject guidance into the conversation)
- `PermissionRequest` is parsed but always **passes through** until Nudge has a permission-specific rule surface
- `Delete` is normalized but not yet matchable from YAML rules

## Keeping Documentation in Sync

Nudge has three documentation sources that must stay aligned. When updating one, consider whether the others need updates too.

| Document | Audience | Purpose | Focus |
|----------|----------|---------|-------|
| **AGENTS.md** | You, developing Nudge | How Nudge works under the hood | Architecture, internals, testing patterns |
| **README.md** | Humans evaluating or contributing | Why Nudge exists and what it believes | Philosophy, motivation, the collaborative framing |
| **`nudge claude docs` / `nudge codex docs`** | You or humans writing rules elsewhere | How to write rules (reference card) | Rule syntax, examples |

**AGENTS.md** (this file) is for *developing* Nudge - understanding the module layout, how to add features, how tests work.

**README.md** is for *understanding* Nudge - the philosophy that Nudge is a collaborative partner, why directness matters, how to write effective rules. This is the front door; it needs to convey the spirit.

**`nudge claude docs` / `nudge codex docs`** (`src/cmd/claude/docs.rs`, `src/cmd/codex/docs.rs`) is for *using* Nudge - a self-contained reference that future agents or humans can consult when writing rules. It should be scannable, copy-pasteable, and not assume any prior context.

When you change something fundamental (like changing the rule format or refining the collaborative framing), update all three.
