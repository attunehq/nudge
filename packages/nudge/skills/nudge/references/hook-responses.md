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

Claude Code currently exposes Nudge-relevant `PreToolUse` surfaces for `Write`,
`Edit`, `WebFetch`, and `Bash`, plus `UserPromptSubmit`.

Codex CLI currently exposes file edits through `apply_patch` normalization,
partial Bash coverage depending on the hook event, and `UserPromptSubmit`.
Codex users should still write file rules in terms of `Write` and `Edit`;
Nudge adapts `apply_patch` internally.

`PermissionRequest` and `Delete` are normalized internally where possible, but
YAML rule matching is intentionally narrower than the internal model until those
surfaces have stable rule semantics.

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

For learned context:

1. Read [learnings.md](learnings.md).
2. Read the cited note before relying on it.
3. Reuse the prior fix only when the situation matches.
