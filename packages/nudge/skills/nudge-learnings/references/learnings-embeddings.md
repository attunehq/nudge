# Nudge Learnings With Local Embeddings

Use this guide when `nudge learn embeddings status` reports both `Embedding
support: available` and `Embeddings: enabled`, or `.nudge.yaml` / `.nudge.yml`
sets `learn.embeddings.enabled: true` and the current binary supports
embeddings.

## Retrieval Behavior

Nudge uses hybrid retrieval: BM25 plus local semantic embeddings. Semantic search
can match paraphrases, but concrete repo terms still improve ranking.

Search with natural symptoms first:

```bash
nudge learn search "Expo fails after dependency update and Metro cannot resolve modules"
nudge learn search "the local embedding model keeps rebuilding the vector index"
nudge learn list
```

If results look stale after editing many notes, rebuild the user-level vector
cache:

```bash
nudge learn embeddings reindex
```

Check cache and model state:

```bash
nudge learn embeddings status
```

Model files and vectors live in the user-level Nudge cache. The repo stores the
Markdown notes, not the generated vectors.

## Acting On Surfaced Context

When hook context says "Nudge found learned repo knowledge":

1. Read the cited note path under `.nudge/learned`.
2. Compare the note's symptoms, environment, and fix to the current task.
3. Apply the fix when it matches.
4. If it does not match, say briefly why and continue investigating.

Embeddings can surface notes with different wording, so verify applicability
before applying a fix.

After bulk importing or heavily editing notes, run
`nudge learn embeddings reindex`.
