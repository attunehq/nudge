---
description: Review this session and record durable Nudge learnings
---

Review the current session history and decide whether anything should become a
repo-local Nudge learned incident note.

Focus on hard-won, repo-specific knowledge that future agents should not have
to rediscover: surprising failures, toolchain quirks, dependency behavior,
environment constraints, fix patterns, and verification commands. Do not record
generic programming advice, secrets, credentials, or one-off observations that
are unlikely to recur.

If the user supplied arguments in `$ARGUMENTS`, use them as the focus area.

For each useful learning, record a note with `nudge learn add`. The note should
include:

- `# <short specific title>`
- `## What went wrong`
- `## Fix`
- `## Verification`

Prefer one concise note per incident. After writing notes, run `nudge learn
search <relevant query>` or `nudge learn list` to confirm they were recorded.

If there is no durable repo-specific learning, say that clearly and do not
create a note.
