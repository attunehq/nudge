---
name: nudge-learnings
description: Use when working in a repository that uses Nudge learned incident notes, when Nudge surfaces learned repo knowledge, when the user asks to search/list/record learnings, or after fixing a repo-specific bug that future agents should not rediscover. Guides use of `nudge learn add`, `nudge learn search`, `nudge learn list`, and local embedding status.
---

# Nudge Learnings

Before using learned notes, choose the retrieval guide:

1. Run `nudge learn embeddings status`, or inspect `.nudge.yaml` / `.nudge.yml` for `learn.embeddings.enabled`.
2. If embeddings are enabled, read [references/embeddings.md](references/embeddings.md).
3. Otherwise, read [references/bm25.md](references/bm25.md).

When Nudge surfaces learned context, treat it as repo memory from a previous debugging session. Read the cited note, decide whether it applies to the current situation, and reuse the fix or explain why the case differs.
