# Nudge Learnings Without Embeddings

Use this guide when `nudge learn embeddings status` reports disabled, or no
`learn.embeddings.enabled: true` appears in `.nudge.yaml` / `.nudge.yml`.

## Retrieval Behavior

Nudge uses BM25 lexical search over `.nudge/learned/*.md`. Exact repo terms
matter: package names, commands, error fragments, paths, tool names, environment
names, and framework names.

Search before repeating debugging work:

```bash
nudge learn search expo metro cannot resolve module
nudge learn search cargo lock fastembed windows
nudge learn list
```

If the first search misses, retry with concrete words copied from the error or
command output. Prefer several focused searches over one vague sentence.

## Acting On Surfaced Context

When hook context says "Nudge found learned repo knowledge":

1. Read the cited note path under `.nudge/learned`.
2. Compare the note's symptoms, environment, and fix to the current task.
3. Apply the fix when it matches.
4. If it does not match, say briefly why and continue investigating.

Because BM25 is lexical, include the real words a future agent will search for
when recording notes.
