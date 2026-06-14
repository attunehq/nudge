# Nudge Learnings

Use learned incident notes when Nudge surfaces learned repo knowledge, when the
user asks to search, list, or record learnings, or after fixing a repo-specific
bug that future agents should not rediscover.

Learned notes live in `.nudge/learned/*.md`. They are repo memory from previous
debugging sessions, not generic advice.

## Choose Retrieval Guide

1. Run `nudge learn embeddings status`, or inspect `.nudge.yaml` / `.nudge.yml`
   for `learn.embeddings.enabled`.
2. If status reports `Embedding support: unavailable in this binary`, read
   [learnings-bm25.md](learnings-bm25.md), even when config enables embeddings.
3. If embeddings are enabled and support is available, read
   [learnings-embeddings.md](learnings-embeddings.md).
4. Otherwise, read [learnings-bm25.md](learnings-bm25.md).

## Commands

```bash
nudge learn search expo metro cannot resolve module
nudge learn list
nudge learn add --title "Expo Metro resolver cache" --body "What went wrong..."
nudge learn embeddings status
```

## Acting On Surfaced Context

When Nudge says it found learned repo knowledge:

1. Read the cited note path under `.nudge/learned`.
2. Compare symptoms, environment, and fix to the current task.
3. Reuse the fix when the situation matches.
4. If the case differs, say why and continue investigating.

Do not route around a relevant note. It exists because an earlier agent already
spent time there.

## Recording A Learning

Record a note after fixing a repo-specific issue another agent could plausibly
hit again. Use this structure:

```markdown
# Short specific title

## What went wrong

Name the command, tool, package, path, error, and symptoms.

## Fix

Give the exact fix and caveats.

## Verification

State the command or observation that proved the fix worked.
```

For longer notes, pipe Markdown:

```bash
cat incident.md | nudge learn add
```
