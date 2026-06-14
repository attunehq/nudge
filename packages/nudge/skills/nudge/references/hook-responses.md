# Nudge Hook Responses

## Response Types

Nudge returns provider-specific hook responses, but the working model is simple:

- Passthrough: no rule or learned note applies. Continue normally.
- Continue: prompt-submission context was injected before the turn starts.
- Learned context: relevant `.nudge/learned/*.md` notes were surfaced.
- Interrupt: a supported pre-tool operation was blocked. Fix the operation and
  retry.
- Warning: Nudge could not fully inspect a supported surface, so it allowed the
  operation and warned the model to tell the user.
- Substitute: Nudge rewrote a deterministic Bash command and allowed the
  rewritten command to run.

## Provider Surfaces

| Surface | Claude Code | Codex CLI |
| --- | --- | --- |
| `PreToolUse Write` | yes | yes, through `apply_patch` add-file parsing |
| `PreToolUse Edit` | yes | yes, through `apply_patch` update parsing |
| `PreToolUse Delete` | normalized | normalized through `apply_patch` delete-file parsing |
| `PreToolUse WebFetch` | yes | no; current Codex hooks do not intercept WebSearch/WebFetch |
| `PreToolUse Bash` | yes | partial; Codex hook coverage is incomplete for some shell paths |
| `PermissionRequest` | parsed only | parsed only |
| `UserPromptSubmit` | yes | yes |

Write YAML rules in terms of `Write`, `Edit`, `WebFetch`, `Bash`, and
`UserPromptSubmit`. Codex `apply_patch` is an adapter detail. `Delete` and
`PermissionRequest` are parsed so Nudge can name them precisely, but they do not
have YAML rule matchers yet.

If Codex file-edit input cannot be parsed safely, Nudge allows the operation
with a model-visible warning. Treat that as "the operation was not fully
inspected", not as proof that the edit satisfied every rule.

## How To Respond

For interrupts:

1. Do not repeat the blocked operation unchanged.
2. Fix the specific content, path, URL, or command Nudge identified.
3. Retry only after the attempted operation satisfies the rule.
4. If the rule appears wrong or stale, say so and update the rule only when that
   is in scope.

For warnings:

1. Continue only if the operation still makes sense.
2. Tell the user when the warning affects confidence, safety, or expected output.
3. Prefer making the operation inspectable when that is practical.

For substitutions:

1. Treat the substituted command as the command that ran.
2. Preserve the original-to-new mapping when summarizing work.
3. Do not rerun the original command unless the user explicitly asks for it.
4. Remember that `nudge check` does not evaluate substitute rules; they require
   a live Bash hook payload.

For learned context:

1. Use `nudge-learnings`.
2. Read the cited note before relying on it.
3. Reuse the prior fix only when the situation matches.

## When Nudge Blocks You

Nudge is a collaborator, not a punishment mechanism. Do not route around it by
renaming files, changing commands, or splitting edits purely to avoid the
matcher. Either satisfy the rule or, when the rule is wrong, fix the rule as
part of the work.

For noisy, silent, or surprising rules, read
[rule-debugging.md](rule-debugging.md).
