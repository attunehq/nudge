---
name: nudge-learnings
description: Use during repo debugging to search `.nudge/learned` before investigating, apply relevant notes, and record durable fixes with `nudge learn add`.
---

# Nudge Learnings

## Purpose

Nudge learnings are repo-local debugging memory. Use this skill proactively
when a repo-specific failure might already have a recorded fix, and after
solving a durable repo-specific issue that future agents should not rediscover.

## When to use

- You are debugging a failing command, test, build, integration, dependency,
  local environment, or toolchain behavior in a repo with `.nudge/learned`.
- Nudge surfaces learned repo context.
- The user asks to search, list, record, or update Nudge learnings.
- You fixed a repo-specific issue that another agent could plausibly hit again.

Do not record generic programming advice, secrets, credentials, or one-off
observations that are unlikely to recur.

## Workflow

1. If `.nudge/learned` exists, search before deep investigation:
   - Use concrete symptoms, command names, package names, paths, tools, and
     copied error fragments.
   - Prefer several focused searches over one vague query.
   - If no learning matches, continue normal debugging.
2. If Nudge already surfaced a note, read the cited `.nudge/learned/*.md` file.
3. Apply a note only when its symptoms, environment, and fix match the current
   task. If it does not match, say why and keep investigating.
4. After fixing a durable repo-specific issue, record a concise note with
   `nudge learn add` and verify it with `nudge learn search` or
   `nudge learn list`.

## Commands

```bash
nudge learn search "exact error text package command"
nudge learn list
nudge learn add --title "Short specific title" --body "What went wrong..."
nudge learn embeddings status
```

## Note Shape

```markdown
# Short specific title

## What went wrong

Name the command, tool, package, path, error, and symptoms.

## Fix

Give the exact fix and caveats.

## Verification

State the command or observation that proved the fix worked.
```

## References

- `references/learnings.md`: choosing retrieval mode and recording notes.
- `references/learnings-bm25.md`: BM25 search behavior and search phrasing.
- `references/learnings-embeddings.md`: hybrid local embedding behavior.
