# Nudge Developer Guide

This guide is for developing Nudge itself. It covers the local architecture,
build and test commands, dogfooding expectations, and live-agent testing flow
that should accompany changes to hooks, setup, rules, learned notes, or agent
guidance.

## Local Setup

Build from the repository root:

```bash
cargo build -p nudge
```

Run the branch binary directly:

```bash
cargo run -p nudge -- --help
```

Put the branch binary first on `PATH` when testing setup or live agents:

```bash
export PATH="$PWD/target/debug:$PATH"
nudge --help
```

Install the branch binary locally only when you intentionally want this checkout
to become your normal `nudge` command:

```bash
make install
```

Nudge is a single-crate Rust workspace under `packages/nudge`.

## Architecture

Important entrypoints:

| Path | Purpose |
| --- | --- |
| `packages/nudge/src/main.rs` | Top-level CLI commands |
| `packages/nudge/src/agent.rs` | Provider selection |
| `packages/nudge/src/agent/claude.rs` | Claude Code hook parsing |
| `packages/nudge/src/agent/codex.rs` | Codex CLI hook parsing and `apply_patch` adaptation |
| `packages/nudge/src/hook.rs` | Provider-neutral hook model |
| `packages/nudge/src/hook/evaluate.rs` | Rule and learned-context evaluation |
| `packages/nudge/src/hook/response.rs` | Provider-specific response rendering |
| `packages/nudge/src/hook/apply_patch.rs` | Codex patch normalization |
| `packages/nudge/src/rules.rs` | Rule discovery and loading |
| `packages/nudge/src/rules/schema.rs` | Rule schema facade |
| `packages/nudge/src/rules/schema/` | Content, glob, syntax, target, URL, and project-state matchers |
| `packages/nudge/src/learn.rs` | Learned incident notes and BM25 retrieval |
| `packages/nudge/src/learn/embeddings.rs` | Local embedding cache and hybrid retrieval |
| `packages/nudge/src/skills.rs` | Bundled skill assets and install helpers |
| `packages/nudge/src/cmd/` | CLI subcommands |
| `packages/nudge/tests/it/` | Integration tests |
| `packages/nudge/skills/nudge/` | Source for the bundled Nudge skill |
| `examples/rules/` | Copyable example rule files |

Core shape:

1. Provider adapters parse raw hook JSON into `NudgeHook`.
2. Rule loading combines user-level config, `.nudge.yaml`, `.nudge.yml`, and
   YAML files under `.nudge/`.
3. Evaluation runs provider-neutral matchers against normalized hook data.
4. Responses render back into the provider's expected format.
5. Learned-note retrieval can add prompt context or allow-with-context warnings.

Delete and PermissionRequest surfaces are normalized internally but do not have
YAML matchers yet. Keep docs, tests, and response rendering explicit when adding
new surfaces.

## Development Loop

Use the narrowest fast check while iterating, then run the full suite before
publishing.

```bash
cargo fmt --all -- --check
cargo clippy -p nudge --all-targets --all-features -- -D warnings
cargo test -p nudge
```

When a change touches release packaging, installer scripts, TLS/download
dependencies, or learned embeddings, also run:

```bash
cargo build -p nudge --release
cargo build -p nudge --no-default-features --release
actionlint
```

The GitHub `Release` workflow runs as a dry-run matrix on pull requests,
merge-queue checks, and pushes to `main`. Tag builds use the same matrix and
cache keys, then sign/notarize macOS binaries and create the draft release.
Supported release targets are macOS, Linux, and Windows x64. Targets whose ONNX
Runtime artifacts are unavailable or do not link in the release cross-toolchain
build with `--no-default-features`, which keeps BM25 learned-note search and
omits local semantic embeddings. Keep that split for Intel macOS, musl Linux,
arm64 GNU Linux, and Windows GNU unless `ort`/FastEmbed support changes or
Nudge grows a different embedding backend.

Useful focused commands:

```bash
cargo test -p nudge learn
cargo test -p nudge skills
cargo test -p nudge setup
cargo test -p nudge codex
cargo test -p nudge markdown
```

Run bundled skill checks and examples when touching rule syntax or user guidance:

```bash
nudge validate
nudge check docs/ README.md
```

Use `nudge syntaxtree` when writing or debugging tree-sitter queries:

```bash
nudge syntaxtree --language rust packages/nudge/src/main.rs
```

## Testing Rules And Hook Behavior

For a single rule:

```bash
nudge test --rule no-inline-imports --tool Write --file test.rs \
  --content $'fn main() {\n    use std::io;\n}'
```

For provider hook shape, pipe representative hook JSON into the provider
subcommand:

```bash
printf '%s\n' '{
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
```

Codex file writes and edits arrive as `apply_patch` commands. Test those through
`nudge codex hook` so the adapter path is exercised:

```bash
printf '%s\n' '{
  "hook_event_name": "PreToolUse",
  "cwd": "/tmp",
  "tool_name": "apply_patch",
  "tool_input": {
    "command": "*** Begin Patch\n*** Add File: test.rs\n+fn main() {\n+    use std::io;\n+}\n*** End Patch\n"
  }
}' | nudge codex hook
```

Expected provider response modes:

- Interrupt/block: exit `0` with provider JSON.
- Substitute: exit `0` with provider JSON containing updated input.
- Continue: exit `0` with plain context.
- Passthrough: exit `0` with no output.
- Configuration or parse failure: non-zero exit with an actionable error.

## Dogfooding While Developing

Nudge is dogfooded in this repository through the committed `.nudge.yaml` rules,
plus any local hooks and user-level rules installed in your environment. Treat
Nudge output as a colleague tapping your shoulder, not an annoyance to route
around. When a Nudge message fires:

1. Follow the instruction if the rule is correct.
2. Improve unclear wording when the rule is right but hard to act on.
3. Fix or remove the rule when the rule is wrong.
4. Mention unrelated rule problems instead of silently broadening the current
   change.

When changing setup, skills, learned notes, or rule syntax, dogfood the branch
binary in disposable repos before trying it in a real project.

## Live-Agent Testing

Use live-agent testing when the change affects agent-visible behavior:

- Hook setup for Claude Code or Codex CLI.
- Provider response JSON.
- Rule messages or snippet rendering.
- Codex `apply_patch` normalization.
- Learned-note prompt or command context.
- Bundled skill install or skill instructions.
- Anything where "the binary works" is insufficient because the question is
  whether a real agent uses the guidance correctly.

The pattern from PR 57 is the preferred template.

### 1. Define The Behavior

Write a short test contract before launching agents:

- What should the agent notice?
- Which command or hook should Nudge run?
- Which file should be created, edited, or avoided?
- Which output proves the agent used Nudge instead of guessing?

Keep the scenario small enough that a failure points at the feature under test.

### 2. Build The Branch Binary

```bash
cargo build -p nudge
export PATH="$PWD/target/debug:$PATH"
nudge --version
```

The `PATH` step matters. It proves setup and agent subprocesses are using the
branch binary, not an older installed release.

### 3. Create A Disposable Git Repo

```bash
tmpdir="$(mktemp -d)"
cd "$tmpdir"
git init
```

Add only the fixtures needed for the scenario: `.nudge.yaml`, `.nudge/learned`
notes, source files, or package manifests. Keep these fixtures in the temp repo
unless they are generally useful integration-test fixtures.

### 4. Run Deterministic CLI Checks First

Exercise the same behavior without a live model:

```bash
nudge validate
nudge check
nudge learn list
nudge learn search expo metro cannot resolve module
nudge learn embeddings status
```

For setup changes:

```bash
nudge claude setup
nudge codex setup
```

Verify generated files directly:

- `.claude/settings.local.json`
- `.codex/hooks.json`
- `.claude/skills/nudge/SKILL.md`
- `.agents/skills/nudge/SKILL.md`

Setup should not create or edit project `CLAUDE.md` or `AGENTS.md`; Nudge
bootstrap guidance lives in the bundled skill.

### 5. Run Codex In The Disposable Repo

Use non-interactive Codex with hooks enabled and the branch binary first on
`PATH`.

```bash
codex exec \
  --cd "$tmpdir" \
  --dangerously-bypass-approvals-and-sandbox \
  --dangerously-bypass-hook-trust \
  --ephemeral \
  --output-last-message "$tmpdir/codex-final.txt" \
  'Use Nudge if it gives you relevant guidance. Report the Nudge commands you ran and the files you changed.'
```

Use `--dangerously-bypass-approvals-and-sandbox` only inside a disposable repo
or another external sandbox. Use `--dangerously-bypass-hook-trust` only for a
hook source you just built and inspected.

Inspect:

```bash
cat "$tmpdir/codex-final.txt"
git -C "$tmpdir" status --short
find "$tmpdir" -maxdepth 4 -type f | sort
```

The final answer should name the Nudge guidance it used. The filesystem should
show the expected result.

### 6. Run Claude Code In The Disposable Repo

Use Claude's non-interactive print mode.

```bash
cd "$tmpdir"
claude --print \
  --permission-mode bypassPermissions \
  --dangerously-skip-permissions \
  --no-session-persistence \
  --output-format text \
  'Use Nudge if it gives you relevant guidance. Report the Nudge commands you ran and the files you changed.'
```

Use bypass permissions only in a disposable repo or another external sandbox.
If a Claude CLI version changes flags, run `claude --help` and keep the same
intent: non-interactive, no durable session, project hooks enabled, and no
permission prompts that hide the hook behavior under test.

### 7. Capture Evidence

Record the commands and the useful output in the PR body. Good evidence names:

- Agent CLI version.
- Branch commit SHA.
- Setup output.
- Nudge command output.
- Agent final text.
- Files created or changed.
- Test suite results.

Avoid vague "tested manually" notes. Reviewers should be able to see what was
proved and why the live-agent run mattered.

### 8. Clean Up

Delete temp repos or keep only small, intentionally committed fixtures. Confirm
the Nudge worktree is clean except for the intended docs/code changes:

```bash
git status -sb
```

## Documentation Rules

Keep these sources aligned when behavior changes:

- `README.md`: landing page and quick orientation.
- `docs/user-guide.md`: user workflow and examples.
- `docs/developer-guide.md`: development, tests, dogfood, and live-agent checks.
- `docs/ci.md`: `nudge check` contract.
- `AGENTS.md`: Codex-facing repository guidance.
- `CLAUDE.md`: Claude-facing repository guidance.
- `packages/nudge/skills/nudge/`: bundled agent-facing Nudge skill and focused
  references for setup, rule writing, debugging, validation, CI, hook
  responses, and learned incident notes.
- `examples/rules/`: copyable starter rules.

When updating docs, preserve plain ASCII quotes and punctuation. The project
expects straight quotes in chat, docs, and code.

## Release Version Contract

`packages/nudge/Cargo.toml` keeps `[package] version = "0.1.0"` as package
metadata only. Nudge is distributed as GitHub release binaries. Release version
truth comes from git tags, `packages/nudge/build.rs`, and GitHub release assets.
Do not treat the Cargo package version as stale metadata unless the project
decides to publish crates.io artifacts.

## Before Opening A PR

Run the checks that match the change:

```bash
cargo fmt --all -- --check
cargo clippy -p nudge --all-targets --all-features -- -D warnings
cargo test -p nudge
```

For docs-only changes, also run:

```bash
nudge check README.md docs/
```

If the docs include command examples, run enough of them to know they still
match the CLI.

For agent-facing changes, include deterministic CLI checks and at least one
live-agent dogfood run for the affected provider. Use both Claude and Codex when
the behavior is provider-neutral.
