---
name: nudge
description: Routes Nudge work to focused guidance for hook responses, rules, CI checks, rule debugging, validation, and learned incident notes. Use when Nudge appears in hook output, blocks, warns, substitutes a command, surfaces learned context, or when the user asks to configure, validate, check, write, debug, or update Nudge rules or learnings.
---

# Nudge

## Purpose

Nudge is a collaborative memory layer for agent hooks. This skill is the router:
pick the focused reference for the Nudge situation, follow it, then return to
the user's task.

## When to use

- Nudge blocks, warns about, or substitutes a tool command.
- Nudge surfaces learned repo context.
- The user asks what Nudge is or why it interrupted.
- The user asks to configure, validate, check, write, debug, or update Nudge
  rules or learned incident notes.
- The user asks to add Nudge to CI, pre-commit, or another scripted gate.
- You find `.nudge.yaml`, `.nudge.yml`, or `.nudge/` while working.

## Workflow

1. Read the Nudge output or user request closely.
2. Choose the most specific reference:
   - Hook responses, interrupts, warnings, substitutions, provider support:
     [references/hook-responses.md](references/hook-responses.md)
   - Writing new rules:
     [references/rule-writing.md](references/rule-writing.md)
   - Noisy, silent, or surprising rules:
     [references/rule-debugging.md](references/rule-debugging.md)
   - Validation commands and check selection:
     [references/validation.md](references/validation.md)
   - CI, pre-commit, release gates, or scripts:
     [references/ci.md](references/ci.md)
   - Learned incident notes:
     [references/learnings.md](references/learnings.md)
3. Follow that reference, then continue the user's task.

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
1. Read `references/learnings.md`.
2. Inspect the cited note.
3. Apply it only if it matches the current situation.

### Example 4

User asks: "Why did Nudge rewrite my command?"

Expected behavior:
1. Read the substitution context from the hook response.
2. If the reason is not obvious, read `references/hook-responses.md`.
3. Explain the original command, the rewritten command, and why the rule prefers
   the rewrite.

### Example 5

User says: "This Nudge rule keeps blocking the wrong thing."

Expected behavior:
1. Read `references/rule-debugging.md`.
2. Reproduce the noisy match with `nudge test`, `nudge check`, or the smallest
   relevant hook payload.
3. Tighten the matcher or message, then rerun the proof command.

### Example 6

User says: "Add Nudge to CI."

Expected behavior:
1. Read `references/ci.md`.
2. Add a scripted `nudge check` gate without depending on live agent hooks.
3. Run the CI command locally when practical.

## Supporting Files

- `references/ci.md`: `nudge check` in CI, pre-commit, and scripted gates.
- `references/hook-responses.md`: provider surfaces and response types.
- `references/learnings.md`: using and recording learned incident notes.
- `references/learnings-bm25.md`: learned-note retrieval without embeddings.
- `references/learnings-embeddings.md`: learned-note retrieval with embeddings.
- `references/rule-writing.md`: rule locations, schema, and examples.
- `references/rule-debugging.md`: diagnosing noisy, silent, or surprising rules.
- `references/validation.md`: choosing `validate`, `test`, and `check`.
