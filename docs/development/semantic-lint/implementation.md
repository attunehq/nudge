# Jev warning-only implementation proof

Date: 2026-09-19. The active worktree's `target/debug/nudge` was run in a
disposable repository using the example rule in `docs/semantic-rules.md`.
Only synthetic Rust was sent to Jev. Credentials were injected from local storage
and are not included in the repository or outputs.

| Scenario | Observed result |
| --- | --- |
| Ordinary redundant loop comment, Claude Write hook | Allow with semantic finding, probability 0.93, 265 ms process wall time |
| Useful connection-pool explanation, Claude Write hook | Allow with uncertainty, probability 0.12, 311 ms process wall time |
| No comments | Passthrough, no model request, 12 ms process wall time |
| `nudge check` with key and redundant comment | Finding, probability 0.92, exit 1 |
| `nudge check` without key | Explicit incomplete check, no all-clear output, exit 2 |

The example clear threshold is 0.1; the useful comment fell just above it.
This is evidence of abstention/noise, not justification to call that comment a
violation or enable blocking. These single-run timings include process startup
and request handling; they do not establish p95 latency or production precision.

Offline tests cover all four provider response paths, exact resulting-file
selection, replace-all and sequential edits, Unicode and Markdown locations,
deterministic short-circuiting, batching, malformed responses, HTTP errors,
redirect rejection, deadlines, and preservation of substitutions with warnings.
Hook-payload tests verify delivery format; they do not prove a live coding agent
will follow every warning.

## Saved credentials and rule severity verification (2026-09-24)

The follow-up implementation adds `nudge login typesafe.ai`, a hidden prompt or
`--stdin`, and user-level `credentials.json`. A key created in the TypeSafe console
was verified and saved with Unix mode 0600. No credential is included in these
artifacts. The tests below ran with `TYPESAFE_API_KEY` unset, so they exercised the
saved credential through the branch binary.

Using the public guide's comment rule and a synthetic redundant loop comment:

| Scenario | Observed result |
| --- | --- |
| `action: warn`, Claude/Codex/Grok/Cursor hook payloads | Allow with finding, probability 0.92-0.93, 189-219 ms |
| `action: block`, the same four provider hook payloads | Deny with finding, probability 0.92-0.93, 167-246 ms |
| `nudge check`, warning rule | Finding, probability 0.93, exit 0 |
| `nudge check`, block rule | Finding, probability 0.93, exit 1 |
| No comments | Exit 0, 13 ms |
| Block rule with clear 0.0 and violation 1.0 | Uncertain at 0.93, exit 2 |

These are individual process wall-time measurements, not p95 or accuracy claims.
Provider payload tests establish response formatting, not coding-agent adherence.
Offline tests additionally cover threshold boundaries, mixed severities,
preconditions on Write/Edit, incomplete-check precedence, credential overrides,
invalid login input, private file permissions, and atomic credential replacement.
