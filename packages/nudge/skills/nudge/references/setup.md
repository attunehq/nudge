# Nudge Setup

Use this reference when a local machine or freshly cloned repo has Nudge rules
but the agent hooks, bundled skill, or Nudge slash commands are not set up yet.
When Nudge has already blocked, warned, substituted, or surfaced learned context, read
[hook-responses.md](hook-responses.md) first.

## When Setup Is Needed

Setup is appropriate when:

- The user asks to install Nudge locally.
- The repo contains `.nudge.yaml`, `.nudge.yml`, or `.nudge/`, but no Nudge hook
  is firing.
- A teammate cloned the repo and needs Claude Code or Codex CLI to honor the
  repo's Nudge rules.
- The bundled `nudge` skill or learning command is missing or stale.

Do not edit `CLAUDE.md`, `AGENTS.md`, or other project instruction files to
teach agents about Nudge. Modern agents should learn Nudge behavior from the
bundled skill and hook responses.

## Install The Binary

First check whether `nudge` is already available:

```bash
nudge --help
```

If it is missing, install the latest release binary.

macOS or Linux:

```bash
curl -sSfL https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/attunehq/nudge/main/scripts/install.ps1 | iex
```

From source:

```bash
git clone https://github.com/attunehq/nudge
cd nudge
cargo install --path packages/nudge
```

After installation, run `nudge --help` again from the user's shell. If the
command still is not found, fix the shell `PATH` before configuring hooks.

## Set Up Project Hooks

Run setup from the project root. Use the command for the agent the user uses, or
both commands when they use both agents:

```bash
nudge claude setup
nudge codex setup
```

Claude setup:

- creates `.claude/` when needed
- writes or merges `.claude/settings.local.json`
- backs up an existing settings file before writing
- registers `PreToolUse` for `Write|Edit|WebFetch|Bash`
- registers `UserPromptSubmit`
- installs the bundled skill to `.claude/skills/nudge`
- installs the Nudge learning slash command to `.claude/commands/nudge/learn.md`

Codex setup:

- creates `.codex/` when needed
- installs the bundled skill to `.agents/skills/nudge`
- installs the Nudge learning prompt command to `.codex/prompts/nudge-learn.md`
- writes or merges `.codex/hooks.json`
- backs up an existing hooks file before writing
- registers `PreToolUse` for `Bash|apply_patch`
- registers `UserPromptSubmit`

If `.codex/config.toml` already contains inline hook tables, Codex setup skips
hook merging and prints a warning. Move those hooks to `.codex/hooks.json` or
merge Nudge manually; Nudge intentionally avoids unsafe TOML hook merges.

## Useful Setup Options

Use non-default project directories only when the repo actually stores agent
configuration somewhere else:

```bash
nudge claude setup --claude-dir path/to/.claude
nudge codex setup --codex-dir path/to/.codex
```

Use `--skip-skills` only when hook setup is wanted but the bundled skill is
managed separately:

```bash
nudge claude setup --skip-skills
nudge codex setup --skip-skills
```

Use `--skip-commands` only when hook setup is wanted but slash commands are
managed separately:

```bash
nudge claude setup --skip-commands
nudge codex setup --skip-commands
```

Reinstall only the bundled skill files when hooks already exist:

```bash
nudge claude skills install
nudge codex skills install
```

For custom skill directories:

```bash
nudge claude skills install --claude-dir path/to/.claude
nudge codex skills install --agents-dir path/to/.agents
```

## Verify Setup

After setup:

1. Restart open Claude Code or Codex sessions so hooks, skills, and commands load.
2. Run `/hooks` in the agent.
3. Run `nudge validate` from the project root.
4. Run `nudge check` or `nudge check <paths>` when file-content rules exist.

Claude-specific checks:

- `claude --debug` shows hook execution logs.
- The Nudge skill should exist at `.claude/skills/nudge`.
- The Nudge learning command should exist at `.claude/commands/nudge/learn.md`.

Codex-specific checks:

- Trust the project `.codex/` layer when prompted.
- Confirm hooks have not been disabled in Codex config.
- The Nudge skill should exist at `.agents/skills/nudge`.
- The Nudge learning prompt command should exist at `.codex/prompts/nudge-learn.md`.

If hooks still do not appear, inspect the generated hook file and verify the
recorded command points to the intended `nudge` binary.
