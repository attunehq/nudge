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
cargo run -p nudge -- claude setup     # Install hooks, bundled skill, and commands for Claude
cargo run -p nudge -- claude skills install # Install the bundled Claude skill
cargo run -p nudge -- codex hook       # Respond to Codex hook (reads JSON from stdin)
cargo run -p nudge -- codex setup      # Install hooks and bundled skill for Codex
cargo run -p nudge -- codex skills install # Install the bundled Codex skill
cargo run -p nudge -- learn add        # Record a repo-local learned incident note
cargo run -p nudge -- learn search     # Search learned incident notes
cargo run -p nudge -- learn embeddings # Manage local learned-note embeddings
cargo run -p nudge -- test             # Test a rule against sample input
cargo run -p nudge -- validate         # Validate rule config files
cargo run -p nudge -- check            # Check project files against rules (for CI)
```

## Release Version Contract

`packages/nudge/Cargo.toml` keeps `[package] version = "0.1.0"` as package
metadata only. This is intentional, not stale metadata.

Nudge is distributed as built GitHub release binaries. The release version
source of truth is the git tag, the built binary version derived by
`packages/nudge/build.rs`, and the GitHub release assets. Do not flag Cargo
package version metadata as a release blocker unless the project explicitly
decides to publish crates.io artifacts.

## Architecture

### CLI Structure

```
nudge claude hook   - Receives hook JSON on stdin, evaluates rules, outputs response
nudge claude setup  - Writes hook configuration and installs the bundled skill and commands for Claude
nudge claude skills install - Installs the bundled skill into .claude/skills
nudge codex hook    - Receives hook JSON on stdin, evaluates rules, outputs response
nudge codex setup   - Writes hook configuration and installs the bundled skill for Codex
nudge codex skills install - Installs the bundled skill into .agents/skills
nudge learn add     - Record a repo-local learned incident note in .nudge/learned
nudge learn list    - List repo-local learned incident notes
nudge learn search  - Search learned incident notes with BM25 or configured local embeddings
nudge learn embeddings - Enable, rebuild, or inspect local learned-note embeddings
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
- `src/learn.rs` - Repo-local learned incident notes and BM25 retrieval
- `src/learn/embeddings.rs` - Local FastEmbed embedding cache and hybrid retrieval
- `src/skills.rs` - Bundled skill and Claude command assets plus installation helpers
- `src/cmd/command_install.rs` - Shared command/prompt install reporting
- `src/cmd/claude/hook.rs` - Hook command: deserializes input, evaluates rules, emits response
- `src/cmd/claude/setup.rs` - Setup command: configures hooks in settings.local.json
- `src/cmd/claude/skills.rs` - Skills command: installs the bundled skill into .claude/skills
- `src/cmd/codex/hook.rs` - Hook command: deserializes input, evaluates rules, emits response
- `src/cmd/codex/setup.rs` - Setup command: configures hooks in hooks.json
- `src/cmd/codex/skills.rs` - Skills command: installs the bundled skill into .agents/skills
- `src/cmd/learn.rs` - CLI for adding, listing, and searching learned notes
- `src/cmd/test.rs` - Test command: test a rule against sample input
- `src/cmd/validate.rs` - Validate command: parse and display rule configs
- `src/cmd/check.rs` - Check command: validate project files against rules for CI
- `packages/nudge/skills/nudge/` - Source files for the bundled Nudge skill compiled into the binary
- `packages/nudge/commands/` - Source files for bundled Claude Nudge slash commands compiled into the binary
- `src/rules.rs` - Rule loading from config files
- `src/rules/schema.rs` - Rule schema facade and hook matcher types
- `src/rules/schema/` - Focused matcher implementations for content, glob paths, project state, tree-sitter syntax, and URLs
- `src/rules/schema/target.rs` - File content targets, including raw content and Markdown code-block extraction
- `src/snippet.rs` - Code snippet rendering for rule violations (uses `annotate-snippets`)

### How Nudge Communicates

When Nudge has something to share, it responds in one of several ways:

- **Passthrough**: Nothing to note. Carry on!
- **Continue**: For UserPromptSubmit hooks, Nudge injects context as plain text
- **Learned context**: For UserPromptSubmit hooks, Nudge searches `.nudge/learned/*.md` with BM25, or hybrid BM25 plus local embeddings when `learn.embeddings.enabled` is set in `.nudge.yaml` or `.nudge.yml`, and injects the most relevant incident notes when the prompt resembles a known scenario. For supported PreToolUse command surfaces, learned context can be surfaced as an allow-with-context warning.
- **Interrupt**: For PreToolUse hooks, Nudge blocks the operation and explains what to fix
- **Warning**: For provider inputs that look like supported PreToolUse surfaces but cannot be inspected (currently Codex apply_patch parse failures), Nudge allows the operation and tells the model to report the warning to the user
- **Substitute**: For deterministic PreToolUse Bash rules, Nudge rewrites the command and lets it proceed

The response type is determined by the hook type:
- `PreToolUse` block rules **interrupt** (block provider-supported Write/Edit/WebFetch/Bash operations)
- `PreToolUse` substitute rules **allow with updated input** (Claude Code and Codex CLI Bash commands)
- `UserPromptSubmit` rules always **continue** (inject guidance into the conversation)
- `PermissionRequest` is parsed but always **passes through** until Nudge has a permission-specific rule surface
- `Delete` is normalized but not yet matchable from YAML rules

Write/Edit file rules evaluate a file content `target` before applying content
matchers. `target: { kind: Content }` is the default raw-content behavior.
`target: { kind: MarkdownCodeBlock, language: rust }` evaluates matchers inside
matching fenced Markdown code blocks and translates match spans back to the
physical Markdown file for snippets and `nudge check` line numbers.

## Keeping Documentation in Sync

Nudge has several documentation sources that must stay aligned. When updating one, consider whether the others need updates too.

| Document | Audience | Purpose | Focus |
|----------|----------|---------|-------|
| **AGENTS.md** | Codex, developing Nudge | Repository guidance for Codex | Architecture, internals, testing patterns |
| **CLAUDE.md** | Claude Code, developing Nudge | Repository guidance for Claude | Architecture, internals, testing patterns |
| **README.md** | Humans evaluating or contributing | Landing page and quick orientation | What Nudge is, why it exists, where to start |
| **docs/user-guide.md** | Users installing and configuring Nudge | How to install, configure, and use Nudge | Setup, expectations, examples, troubleshooting |
| **docs/developer-guide.md** | Contributors changing Nudge | How to develop and dogfood Nudge | Architecture, tests, live-agent validation |
| **packages/nudge/skills/nudge/** | Agents using Nudge | How to respond to Nudge, set it up locally, and write/debug rules | Setup, rule syntax, hook responses, validation, CI, learnings |

**AGENTS.md** (this file) is Codex-facing bootstrap guidance for developing Nudge.

**CLAUDE.md** is Claude-facing bootstrap guidance for developing Nudge.

**README.md** is the front door: a concise landing page that explains what Nudge is and points to the deeper guides.

**docs/user-guide.md** and **docs/developer-guide.md** are the durable guides for users and contributors.

**packages/nudge/skills/nudge/** is the self-contained agent-facing Nudge reference. It should stay scannable, copy-pasteable, and complete enough for modern agents to set up local hooks when needed, understand hook responses, write rules, debug rules, validate changes, add CI gates, and use learned incident notes without relying on CLI docs subcommands.

When you change something fundamental, such as changing the rule format, setup flow, learned-note behavior, or collaborative framing, update every affected source.
