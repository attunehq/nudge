# Nudge

Nudge is a collaborative memory layer for Claude Code and Codex CLI hooks. It
remembers the coding conventions, workflow preferences, and repo-local debugging
lessons that agents should use while they work.

Write the reminder once. Let Nudge deliver it at the moment an agent is about to
write a file, run a command, fetch a URL, or start a turn.

## Why Use It

Agents do better work when they can focus on the actual task instead of holding
every project preference in working memory. Nudge moves those preferences into
small, testable rules and learned notes:

- Rules catch deterministic conventions before an operation lands.
- Bash substitutions rewrite simple command mistakes automatically.
- Prompt reminders add project context when a user asks for something specific.
- Learned incident notes keep future agents from rediscovering old debugging
  fixes.
- `nudge check` brings the same file-rule enforcement to CI and scripts.

Nudge is direct by design. A good message says what is wrong, how to fix it, and
that the agent should retry.

## Quick Start

Install:

```bash
curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash
```

Release binaries support macOS, Linux, and Windows x64. BM25 learned-note
search is always available. Local semantic embeddings are included on Apple
Silicon macOS and x64 GNU Linux; Intel macOS, musl Linux, arm64 GNU Linux, and
Windows GNU builds omit local semantic embeddings.

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
```

Set up hooks in a project. Run the setup command for the agent you use, or both
commands if you use both agents:

```bash
nudge claude setup
nudge codex setup
```

Add a `.nudge.yaml`:

```yaml
version: 1
rules:
  - name: no-unwrap
    message: 'Use `.expect("descriptive error message")` instead of `.unwrap()`, then retry.'
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: Regex
            pattern: "\\.unwrap\\(\\)"
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: Regex
            pattern: "\\.unwrap\\(\\)"
```

Restart open agent sessions, then use Claude Code or Codex CLI normally. Run
`/hooks` in the agent to verify setup. After a useful debugging session, ask the
agent to use Nudge to record durable repo-local learnings for future work; Claude
setup also installs a `nudge:learn` slash command for this workflow.

## Guides

- [User Guide](docs/user-guide.md): install, setup, rule examples, learned
  notes, agent behavior, and troubleshooting.
- [Developer Guide](docs/developer-guide.md): architecture, local development,
  tests, dogfooding, live-agent testing, and PR expectations.
- [CI and Programmatic Checks](docs/ci.md): using `nudge check` in CI,
  pre-commit hooks, and scripts.

The bundled `nudge` skill is the agent-facing rule reference. It includes the
former CLI docs content as focused references for rule writing, debugging,
validation, CI, local setup, hook responses, and learned incident notes.

Copyable starter rules live in [examples/rules](examples/rules).

## How It Works

Nudge watches supported hook surfaces:

- `PreToolUse` for file writes/edits, Bash, and WebFetch where the provider
  exposes them.
- `UserPromptSubmit` for prompt-time context.
- Codex `apply_patch` inputs, normalized into Write/Edit/Delete where possible.

When a rule or learned note matches, Nudge returns one of these outcomes:

| Outcome | Meaning |
| --- | --- |
| Passthrough | Nothing matched, so the agent continues silently. |
| Continue | Prompt-time context is injected into the conversation, including prompt-matched learned notes. |
| Interrupt | A tool call is blocked with a rule message and snippet. |
| Substitute | A deterministic Bash command is rewritten and allowed. |
| Warning | An operation is allowed with model-visible context, such as PreToolUse learned-note context or an uninspectable Codex patch warning. |

Rules are loaded from user-level config, `.nudge.yaml`, `.nudge.yml`, and YAML
files under `.nudge/`. Learned notes are plain Markdown files under
`.nudge/learned/`.

## Learned Notes

Rules are best for deterministic conventions. Learned notes are for incidents:
what went wrong, how it was fixed, and how the fix was verified.

```bash
nudge learn add --title "Expo Metro resolver cache" --body "What went wrong: Expo could not resolve modules after a dependency update.

Fix: clear the Metro cache and restart the dev server.

Verification: expo start completed and the app loaded."

nudge learn search expo metro cannot resolve module
nudge learn list
```

BM25 search is always available. Projects can opt into local semantic retrieval:

```bash
nudge learn embeddings enable
nudge learn embeddings status
```

Setup installs the bundled `nudge` skill so agents know how to respond to hook
messages, write or debug rules, wire CI checks, and search, apply, or record
repo-local learnings. Setup does not edit project `CLAUDE.md` or `AGENTS.md`
files.

## Development

Build and test:

```bash
cargo build -p nudge
cargo test -p nudge
cargo clippy -p nudge --all-targets --all-features -- -D warnings
```

See the [Developer Guide](docs/developer-guide.md) for the full development and
live-agent dogfood workflow.
