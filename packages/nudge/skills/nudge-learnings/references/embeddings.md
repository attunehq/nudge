# Nudge Learnings With Local Embeddings

Use this guide when `nudge learn embeddings status` reports enabled, or `.nudge.yaml` / `.nudge.yml` sets `learn.embeddings.enabled: true`.

## Retrieval behavior

Nudge uses hybrid retrieval: BM25 plus local semantic embeddings. Semantic search can match paraphrases, but concrete repo terms still improve ranking.

Search with natural symptoms first:

```bash
nudge learn search "Expo fails after dependency update and Metro cannot resolve modules"
nudge learn search "the local embedding model keeps rebuilding the vector index"
nudge learn list
```

If results look stale after editing many notes, rebuild the user-level vector cache:

```bash
nudge learn embeddings reindex
```

Check cache and model state:

```bash
nudge learn embeddings status
```

Model files and vectors live in the user-level Nudge cache. The repo stores the Markdown notes, not the generated vectors.

## Acting on surfaced context

When hook context says "Nudge found learned repo knowledge":

1. Read the cited note path under `.nudge/learned`.
2. Compare the note's symptoms, environment, and fix to the current task.
3. Apply the fix when it matches.
4. If it does not match, say briefly why and continue investigating.

Embeddings can surface notes with different wording, so verify applicability before applying a fix.

## Recording a learning

Write notes in plain language. You do not need tags or trigger phrases, but include exact repo terms alongside the story:

```bash
nudge learn add --title "Expo Metro resolver cache" --body "What went wrong: Expo could not resolve modules after a dependency update.

Fix: clear the Metro cache and restart the dev server.

Verification: expo start completed and the app loaded."
```

For longer notes, pipe Markdown:

```bash
cat incident.md | nudge learn add
```

Use this structure:

```markdown
# Short specific title

## What went wrong

Describe the failure in normal debugging language. Include commands, packages, paths, and errors.

## Fix

Give the exact fix and caveats.

## Verification

State the command or observation that proved the fix worked.
```

After bulk importing or heavily editing notes, run `nudge learn embeddings reindex`.
