# RFC 0002: Claude Code and Codex Hook Support

## Summary

Update Nudge from a Claude Code-specific hook adapter into a small agent-hook
platform that supports both Claude Code and Codex CLI.

The clean architecture is:

1. Parse each agent's hook wire format at the CLI boundary.
2. Normalize supported events into Nudge's own event model.
3. Evaluate existing Nudge rules against the normalized event.
4. Render an agent-specific hook response at the boundary.

This preserves the rule language users already write while making the hook
integration explicit, testable, and extensible.

## Research Snapshot

Research date: 2026-05-27.

Primary sources:

- Codex hooks guide: <https://developers.openai.com/codex/hooks>
- Codex config reference: <https://developers.openai.com/codex/config-reference>
- Claude Code hooks reference: <https://code.claude.com/docs/en/hooks>

### Codex Hooks

Codex hooks are enabled by default and can be disabled with:

```toml
[features]
hooks = false
```

Codex discovers hook configuration in `hooks.json` files or inline `[hooks]`
tables next to active config layers. The practical project-local locations are:

- `.codex/hooks.json`
- `.codex/config.toml`

Codex also supports user-level equivalents under `~/.codex`. Project-local
hooks only load when the project `.codex/` layer is trusted. Non-managed command
hooks must be reviewed and trusted with `/hooks` before they run.

The config shape mirrors Claude's three-level model:

- Event name, such as `PreToolUse` or `UserPromptSubmit`
- Matcher group
- One or more hook handlers

Command hooks receive one JSON object on stdin. The events Nudge should install
and evaluate first are:

- `PreToolUse`
- `UserPromptSubmit`

For `PreToolUse`, Codex currently intercepts Bash, `apply_patch`, and MCP tool
calls. It does not currently intercept all shell paths under unified exec, and
it does not intercept WebSearch or other non-shell, non-MCP tools.

Codex also supports `PermissionRequest` for approval prompts. Nudge should
parse that event into its normalized model in this update, but it should not
install `PermissionRequest` hooks or evaluate rules against approval prompts
until Nudge has a permission-specific rule surface.

Codex `PreToolUse` input for Bash and `apply_patch` uses:

- `hook_event_name: "PreToolUse"`
- `tool_name: "Bash"` or `"apply_patch"`
- `tool_input.command`

For `apply_patch`, matcher aliases can include `Edit` or `Write`, but the hook
input still reports `tool_name: "apply_patch"`.

Codex `UserPromptSubmit` input includes:

- `hook_event_name: "UserPromptSubmit"`
- `prompt`

Codex `PreToolUse` denial should be returned through the hook-specific shape:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Destructive command blocked by hook."
  }
}
```

Codex `PreToolUse` substitution should use `permissionDecision: "allow"` with
`updatedInput`, and should include `hookSpecificOutput.additionalContext` when
Nudge needs the model to know what was rewritten:

```json
{
  "systemMessage": "Nudge substituted `npm install foo` -> `yarn add foo`.",
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "updatedInput": {
      "command": "yarn add foo"
    },
    "additionalContext": "Nudge rewrote the Bash command from `npm install foo` to `yarn add foo` before execution."
  }
}
```

Important compatibility detail: Codex documents `continue`, `stopReason`, and
`suppressOutput` as unsupported for `PreToolUse`. Returning those fields on
`PreToolUse` makes the hook run fail and Codex continues the tool call. Nudge's
current Claude response envelope includes those fields, so Codex needs a
separate response renderer or a reduced common renderer that omits unsupported
fields.

For `UserPromptSubmit`, plain stdout is added as extra developer context. JSON
with `hookSpecificOutput.additionalContext` is also supported.

### Claude Code Hooks

Claude Code discovers hooks in:

- `~/.claude/settings.json`
- `.claude/settings.json`
- `.claude/settings.local.json`
- managed policy settings
- plugin `hooks/hooks.json`
- skill or agent frontmatter while active

Claude uses the same broad three-level shape:

- Event name
- Matcher group
- Hook handler

Claude supports more hook handler types than Codex today: `command`, `http`,
`mcp_tool`, `prompt`, and `agent`. Nudge should continue installing only command
hooks.

Claude's matcher behavior differs from Codex:

- `"*"`, `""`, or missing means match all.
- Simple strings with `|` are exact alternatives.
- Values containing other characters are JavaScript regular expressions.
- Tool events match on `tool_name`.

Claude supports the Nudge-relevant tool names directly:

- `Write`
- `Edit`
- `WebFetch`
- `Bash`

Claude also exposes MCP tools in hook events. Nudge should preserve those as
`Other`, matching the existing behavior for unsupported tool names.

Claude `PreToolUse` denial supports the same hook-specific output Nudge already
uses:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Database writes are not allowed"
  }
}
```

Claude also accepts extra common fields, but Nudge does not need them for
blocking a tool call.

For `UserPromptSubmit`, plain stdout is added to Claude's context.

## Prior Nudge Shape

Before this RFC, the public CLI was Claude-shaped:

- `nudge claude hook`
- `nudge claude setup`
- generated rule docs were exposed through the Claude namespace at the time

The Rust model was also Claude-shaped:

- `packages/nudge/src/cmd/claude/hook.rs`
- `packages/nudge/src/cmd/claude/setup.rs`

The rule engine itself is mostly generic. Rules already name lifecycle concepts
instead of Claude-specific APIs:

- `PreToolUse`
- `UserPromptSubmit`
- `Write`
- `Edit`
- `WebFetch`
- `Bash`

That is the right user-facing vocabulary. The problem is that internal payload
types and response serialization are coupled to Claude's wire format.

## Goals

- Support Claude Code and Codex CLI with one rule language.
- Keep `PreToolUse` and `UserPromptSubmit` as the first rule-evaluated events.
- Preserve existing rule files without requiring users to fork rules per agent.
- Correctly block Codex `PreToolUse` calls by avoiding unsupported response
  fields.
- Make Codex `apply_patch` useful for existing `Write` and `Edit` file-content
  rules.
- Model file deletion as a first-class normalized tool event.
- Parse `PermissionRequest` into a normalized event for both agents, without
  making it rule-matchable in this update.
- Keep setup idempotent and avoid clobbering unrelated user settings.
- Keep setup local to each agent's standard local hook configuration.
- Update README, command docs, and repository agent instructions together.

## Non-Goals

- Support every Claude hook event in this update.
- Support every Codex hook event in this update.
- Match rules against `PermissionRequest` or make approval decisions from
  Nudge rules.
- Add YAML rule matching for file deletion.
- Add natural-language or model-judged rules.
- Implement HTTP, MCP-tool, prompt, or agent hook handlers.
- Claim Codex can enforce WebSearch/WebFetch rules when current Codex hooks do
  not intercept that path.

## Design

### CLI Surface

Add a Codex namespace alongside the existing Claude namespace:

```bash
nudge claude hook
nudge claude setup
nudge claude skills install

nudge codex hook
nudge codex setup
nudge codex skills install
```

### Internal Modules

Replace the Claude-specific hook domain model with an agent-neutral one:

```text
packages/nudge/src/agent.rs
packages/nudge/src/agent/claude.rs
packages/nudge/src/agent/codex.rs
packages/nudge/src/hook.rs
packages/nudge/src/hook/evaluate.rs
packages/nudge/src/hook/response.rs
packages/nudge/src/hook/apply_patch.rs
```

Suggested responsibilities:

- `agent/claude.rs`: parse Claude hook JSON.
- `agent/codex.rs`: parse Codex hook JSON, emit Codex setup JSON.
- `hook.rs`: normalized Nudge event types.
- `hook/evaluate.rs`: rule evaluation over normalized events.
- `hook/response.rs`: abstract Nudge outcomes plus provider-specific rendering.
- `hook/apply_patch.rs`: parse Codex `apply_patch` commands into file changes.

Keep provider-specific parsing at the agent boundary. Rule evaluation should
stay centered on normalized events instead of growing provider-specific hook
models.

### Normalized Event Model

Define a normalized event enum:

```rust
pub enum NudgeHook {
    PreToolUse(PreToolUse),
    PermissionRequest(PermissionRequest),
    UserPromptSubmit(UserPromptSubmit),
    Other,
}
```

Define `PreToolUse` around what rules need:

```rust
pub struct PreToolUse {
    pub context: HookContext,
    pub tool: ToolUse,
}

pub struct PermissionRequest {
    pub context: HookContext,
    pub tool: ToolUse,
}

pub enum ToolUse {
    Write(WriteInput),
    Edit(EditInput),
    Delete(DeleteInput),
    WebFetch(WebFetchInput),
    Bash(BashInput),
    Other { tool_name: String, input: serde_json::Value },
}
```

`PermissionRequest` exists so the CLI boundary can parse current agent hook
payloads without treating them as unknown JSON. Rule evaluation must return
passthrough for `PermissionRequest` in this RFC. Nudge should not approve,
deny, rewrite, or add context to approval prompts until the rule language has a
clear permission-specific policy surface.

`HookContext` should include only shared or useful metadata:

```rust
pub struct HookContext {
    pub agent: AgentKind,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub transcript_path: Option<PathBuf>,
    pub cwd: PathBuf,
    pub permission_mode: Option<String>,
    pub model: Option<String>,
}
```

Use `Option` for fields that differ across providers. Parse at the boundary and
avoid sprinkling provider checks through rule evaluation.

### Provider Mapping

Claude maps directly:

| Claude input | Normalized event |
| --- | --- |
| `PreToolUse` + `tool_name: "Write"` | `ToolUse::Write` |
| `PreToolUse` + `tool_name: "Edit"` | `ToolUse::Edit` |
| `PreToolUse` + `tool_name: "WebFetch"` | `ToolUse::WebFetch` |
| `PreToolUse` + `tool_name: "Bash"` | `ToolUse::Bash` |
| `PreToolUse` + MCP tool | `ToolUse::Other` |
| `PermissionRequest` | `NudgeHook::PermissionRequest`, parsed but unmatchable |
| `UserPromptSubmit` | `NudgeHook::UserPromptSubmit` |

Codex maps as follows:

| Codex input | Normalized event |
| --- | --- |
| `PreToolUse` + `tool_name: "Bash"` | `ToolUse::Bash` from `tool_input.command` |
| `PreToolUse` + `tool_name: "apply_patch"` add file | `ToolUse::Write` per added file |
| `PreToolUse` + `tool_name: "apply_patch"` update file | `ToolUse::Edit` per updated file |
| `PreToolUse` + `tool_name: "apply_patch"` delete file | `ToolUse::Delete` per deleted file |
| `PreToolUse` + MCP tool | `ToolUse::Other` |
| `PermissionRequest` | `NudgeHook::PermissionRequest`, parsed but unmatchable |
| `UserPromptSubmit` | `NudgeHook::UserPromptSubmit` |

Codex `apply_patch` can contain multiple file changes. A single raw hook may
normalize to multiple Nudge `PreToolUse` events. Evaluate all of them and emit
one response that aggregates all matches.

### Codex `apply_patch` Handling

Codex file edits generally arrive as `apply_patch`, so this is required for
existing file-content rules to work.

Implement a small parser for the patch format used by the `apply_patch` tool:

- `*** Begin Patch`
- `*** Add File: <path>`
- `*** Update File: <path>`
- `*** Delete File: <path>`
- optional `*** Move to: <path>`
- `*** End Patch`

For added files:

- Collect added lines.
- Normalize into `WriteInput { file_path, content }`.

For updated files:

- Read the current file from `cwd`.
- Apply hunks in memory.
- Normalize into `EditInput { file_path, old_string, new_string }`, where
  `new_string` is the full post-patch file content.

For moved files:

- Use the destination path for file glob matching.

For deleted files:

- Normalize into `DeleteInput { file_path }`.
- This is a first-class `ToolUse::Delete` event, but it is not matchable from
  YAML in this update. A deleted file is not an "other" tool use; it is a
  concrete file operation that future rules and audit output can name
  precisely.

If patch parsing fails:

- Do not block the tool call.
- Emit a tracing warning.
- In debug mode, tell the user Nudge could not inspect that patch.

Fail-open is appropriate here because Nudge is a collaborative reminder layer.
Blocking every unparseable patch would make it feel hostile.

### Rule Evaluation

Move rule evaluation out of `cmd/claude/hook.rs` into a provider-neutral
function:

```rust
pub fn evaluate_hook(hook: &NudgeHook, rules: &[Rule]) -> Evaluation;
```

`Evaluation` should collect:

- matched annotations
- source text for snippet rendering
- matched rule names
- desired Nudge outcome

The existing behavior remains:

- `PreToolUse` block matches produce a blocking denial.
- `PreToolUse` Bash substitute matches produce an allow response with updated input.
- `UserPromptSubmit` matches produce context on stdout.
- `PermissionRequest` always passes through in this update.
- No matches produce no output and exit 0.

`ToolUse::Delete` is also unmatchable in this update. It should be preserved in
the normalized model and test coverage, but rule evaluation should not expose a
YAML matcher for it until Nudge has a concrete deletion policy to express.

### Response Rendering

Define an abstract outcome:

```rust
pub enum HookOutcome {
    Passthrough,
    DenyPreToolUse { message: String },
    UpdatePreToolUse {
        system_message: String,
        additional_context: String,
        updated_input: Value,
    },
    AddContext { context: String },
}
```

Render it per agent:

Claude `DenyPreToolUse`:

```json
{
  "systemMessage": "Nudge blocked operation due to rule violation.",
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "<message>"
  }
}
```

Codex `DenyPreToolUse`:

```json
{
  "systemMessage": "Nudge blocked operation due to rule violation.",
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "<message>"
  }
}
```

Do not include `continue`, `stopReason`, or `suppressOutput` on `PreToolUse`.
This is the key wire-format fix for Codex.

For `UpdatePreToolUse`, render `permissionDecision: "allow"` with a full
`updatedInput` object. Preserve every original tool input field and replace
only the rewritten field. Put the model-visible explanation in
`hookSpecificOutput.additionalContext`; use `systemMessage` only as an
audit/user-visible note.

For `UserPromptSubmit`, prefer plain stdout for both agents. It is simple and
documented by both hook systems as model-visible context.

For `PermissionRequest`, render `Passthrough` only. The hook command should
exit 0 with no output for that event.

### Setup

Claude setup should continue writing `.claude/settings.local.json`, but it
should only install events Nudge actually handles:

- `PreToolUse`
- `UserPromptSubmit`

The current setup also installs `PostToolUse` and `Stop`, but the hook command
passes those through. Remove them. This is a behavior change, but it reduces
unnecessary hook executions and makes setup honest.

Claude `PreToolUse` matcher should be narrower than `"*"`:

```json
{
  "matcher": "Write|Edit|WebFetch|Bash",
  "hooks": [
    {
      "type": "command",
      "command": "<nudge> claude hook",
      "timeout": 5
    }
  ]
}
```

Codex setup should write `.codex/hooks.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash|apply_patch",
        "hooks": [
          {
            "type": "command",
            "command": "<nudge> codex hook",
            "timeout": 5,
            "statusMessage": "Checking Nudge rules"
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "<nudge> codex hook",
            "timeout": 5,
            "statusMessage": "Checking Nudge rules"
          }
        ]
      }
    ]
  }
}
```

Use local `.codex/hooks.json` rather than inline `[hooks]` in
`.codex/config.toml` because Nudge already has safe JSON merge behavior and
Codex warns when a single layer contains both representations. Do not add a
committed/shared Codex setup mode in this update. If `.codex/config.toml`
already contains inline hooks, setup should warn and skip automatic merge
because TOML-preserving inline hook editing is out of scope.

Codex setup should print these next steps:

1. Restart Codex sessions so hooks are loaded.
2. Run `/hooks`.
3. Review and trust the new Nudge hooks.
4. If hooks do not appear, check that the project `.codex/` layer is trusted and
   `[features].hooks` has not been disabled.

### Documentation

Update these together:

- `README.md`
- `AGENTS.md`
- `CLAUDE.md`
- `packages/nudge/skills/nudge/`

README should describe Nudge as supporting "agent hooks" and then name the
current integrations:

- Claude Code
- Codex CLI

The rule writing guide should state provider support per surface:

| Surface | Claude Code | Codex |
| --- | --- | --- |
| `PreToolUse Write` | yes | yes, through `apply_patch` add-file parsing |
| `PreToolUse Edit` | yes | yes, through `apply_patch` update parsing |
| `PreToolUse Delete` | normalized only, no YAML matcher yet | normalized only, through `apply_patch` delete-file parsing, no YAML matcher yet |
| `PreToolUse WebFetch` | yes | no, current Codex hooks do not intercept WebSearch |
| `PreToolUse Bash` | yes | partial, current Codex hook coverage is documented as incomplete for some unified exec paths |
| `PermissionRequest` | parsed only, no YAML matcher yet | parsed only, no YAML matcher yet |
| `UserPromptSubmit` | yes | yes |

Do not document `apply_patch` as a user-facing rule target. It is a significant
mental model departure from Nudge's rule vocabulary, so the Codex adapter should
translate it into `Write`, `Edit`, and `Delete` before rules see it.

### Validation and Warnings

`nudge validate` should warn when a project appears to target Codex and contains
rules that cannot fire there, especially `WebFetch`.

The warning should be informational, not a failure:

```text
warning: rule "prefer-local-docs" uses PreToolUse WebFetch, which Claude Code
supports but Codex hooks do not currently intercept.
```

`nudge check` only validates repository files today. Keep Codex patch payload
evaluation out of `check`; hook payload behavior belongs in hook command tests
and `nudge test`.

### Tests

Add provider-neutral unit tests:

- Claude `Write` payload normalizes to `ToolUse::Write`.
- Claude `Edit` payload normalizes to `ToolUse::Edit`.
- Claude `Bash` payload normalizes to `ToolUse::Bash`.
- Claude `PermissionRequest` normalizes to `NudgeHook::PermissionRequest` and
  passes through.
- Claude `UserPromptSubmit` normalizes to prompt text.
- Codex `Bash` payload normalizes to `ToolUse::Bash`.
- Codex `PermissionRequest` normalizes to `NudgeHook::PermissionRequest` and
  passes through.
- Codex `UserPromptSubmit` normalizes to prompt text.
- Codex `apply_patch` add-file normalizes to `ToolUse::Write`.
- Codex `apply_patch` update-file normalizes to `ToolUse::Edit`.
- Codex `apply_patch` delete-file normalizes to `ToolUse::Delete`.
- Codex multi-file patch aggregates all matches into one denial.
- Codex unsupported MCP tool passes through.
- `ToolUse::Delete` does not match any YAML rule in this update.

Add response renderer tests:

- Claude denial JSON contains `permissionDecision: "deny"`.
- Codex denial JSON contains `permissionDecision: "deny"`.
- Codex denial JSON does not contain `continue`, `stopReason`, or
  `suppressOutput`.
- PermissionRequest renders no output for both agents.
- UserPromptSubmit context renders as plain text for both agents.

Add setup tests:

- `nudge claude setup` is idempotent.
- `nudge codex setup` creates `.codex/hooks.json`.
- `nudge codex setup` is idempotent.
- Existing unrelated hook config is preserved.
- Existing inline Codex hooks in `.codex/config.toml` produce a warning rather
  than an unsafe merge.

Add integration tests:

- Existing Claude integration tests still pass via `nudge claude hook`.
- Equivalent Codex tests pass via `nudge codex hook`.
- A Codex `apply_patch` payload that writes Rust with an inline import is
  blocked by the existing `no-inline-imports` rule.

## Migration Plan

1. Introduce normalized hook types and move evaluation into provider-neutral
   code.
2. Rewire `nudge claude hook` through the normalized path with no intended
   behavior change except the reduced PreToolUse JSON envelope.
3. Add `nudge codex hook` and Codex response rendering.
4. Add Codex `apply_patch` parsing and multi-change aggregation.
5. Add parsed-but-unmatchable `PermissionRequest` support for both agents.
6. Add `nudge codex setup`.
7. Narrow Claude setup to the supported events and tools.
8. Update docs and repository instructions.
9. Run:

```bash
cargo fmt --all
cargo test -p nudge
cargo run -p nudge -- validate
cargo run -p nudge -- check README.md docs/ AGENTS.md CLAUDE.md packages/nudge/skills/
```

## Breaking Changes

- `nudge claude setup` should stop installing `PostToolUse` and `Stop` because
  Nudge does not currently handle those events.
- `nudge codex setup` should only write local `.codex/hooks.json`; it should not
  add a committed/shared setup mode.
- PreToolUse response JSON should omit common fields that Nudge does not need.
  Claude should still honor the denial, and Codex requires this cleaner shape.
- Documentation should stop describing Nudge as Claude-only.
