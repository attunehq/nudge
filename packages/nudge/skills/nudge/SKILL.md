---
name: nudge
description: Explains Nudge hook responses and rule-writing. Use when Nudge appears in hook output, blocks, warns, or substitutes a command, surfaces rule guidance, or when the user asks to configure, validate, check, write, debug, or update Nudge rules. Do not use for repo-local learned incident notes; use nudge-learnings.
---

# Nudge

## Purpose

Nudge is a collaborative memory layer for agent hooks. It reminds agents about
repo conventions, blocks or rewrites supported operations when rules match, and
surfaces relevant repo-local learned incidents. Treat Nudge messages as useful
context from the project, not as an error to route around.

## When to use

- Nudge blocks, warns about, or substitutes a tool command.
- The user asks what Nudge is or why it interrupted.
- The user asks to configure, validate, check, write, debug, or update Nudge
  rules.
- You find `.nudge.yaml`, `.nudge.yml`, or `.nudge/` while working.

## Do not use when

- The task is specifically about searching, applying, or recording learned
  incident notes. Use the `nudge-learnings` skill for that workflow.
- The user is asking about unrelated agent skills or general hook systems.

## Workflow

1. If Nudge interrupted a command, read the whole message and any snippet. Fix
   the attempted operation, then retry the corrected operation.
2. If Nudge allowed the operation with a warning, report the warning when it is
   user-visible or affects confidence, then continue with the warning in mind.
3. If Nudge substituted a Bash command, treat the rewritten command as the one
   that ran and preserve that fact in any summary.
4. If the message mentions learned context or `.nudge/learned`, switch to the
   `nudge-learnings` skill before using those notes.
5. If you are writing or debugging rules, read
   [references/rule-writing.md](references/rule-writing.md), then use
   [references/validation.md](references/validation.md) for the check sequence.
6. If the task is specifically about validating Nudge config or checking files,
   read [references/validation.md](references/validation.md).
7. If you need to explain hook behavior, response types, or provider support,
   read [references/hook-responses.md](references/hook-responses.md).

## Examples

### Example 1

Nudge blocks an edit that adds a forbidden pattern.

Expected behavior:
1. Read the Nudge message and snippet.
2. Change the edit to satisfy the rule.
3. Retry the operation.
4. Mention the rule only if it matters to the user-facing summary.

### Example 2

User says: "Add a Nudge rule that blocks npm install."

Expected behavior:
1. Read `references/rule-writing.md`.
2. Add or update the appropriate `.nudge.yaml` or `.nudge/*.yaml` rule.
3. Read `references/validation.md`.
4. Run the checks that prove the new rule parses and behaves as intended.

### Example 3

Nudge surfaces learned context from `.nudge/learned`.

Expected behavior:
1. Switch to the `nudge-learnings` skill.
2. Inspect the cited note.
3. Apply it only if it matches the current situation.

### Example 4

User asks: "Why did Nudge rewrite my command?"

Expected behavior:
1. Read the substitution context from the hook response.
2. If the reason is not obvious, read `references/hook-responses.md`.
3. Explain the original command, the rewritten command, and why the rule prefers
   the rewrite.

## Supporting Files

- `references/hook-responses.md`: provider surfaces and response types.
- `references/rule-writing.md`: rule locations, schema, and examples.
- `references/validation.md`: choosing `validate`, `test`, and `check`.
