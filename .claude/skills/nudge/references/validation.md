# Nudge Validation

Use these commands after changing Nudge rules or files that should satisfy
Nudge rules.

## Commands

```bash
nudge validate
nudge test
nudge check
nudge check src/ docs/
```

## Which Check To Run

Run `nudge validate` after changing Nudge config. It proves the rule files parse,
unknown fields are rejected, and provider support warnings are visible.

Run `nudge test` when you need a focused sample input for one rule. This is the
fastest way to prove a new matcher fires or stays quiet for a specific case.

Run `nudge check` after changing files that should satisfy file-content rules.
Pass explicit paths when the affected area is known; use the default project
scan when broad coverage matters more than speed.

Run the normal project test or lint command as well when the Nudge change is
paired with source changes. Nudge validates conventions; it does not replace the
project's own correctness checks.

## Suggested Sequence

For rule-only changes:

1. Run `nudge validate`.
2. Run `nudge test` if there is a small sample that proves the rule.
3. Run `nudge check` for affected files when the rule targets file content.

For source changes that were prompted by a Nudge message:

1. Retry the corrected operation so the hook can evaluate it.
2. Run `nudge check <paths>` if the rule can also be checked outside live hooks.
3. Run the project-specific tests for the changed code.

For docs or bundled skill changes:

1. Run `nudge validate`.
2. Run `nudge check README.md docs/ AGENTS.md CLAUDE.md packages/nudge/skills/`
   when those paths exist in this repository.
3. Run the package or project tests that cover bundled asset installation.

## Interpreting Results

If `nudge validate` warns about provider support, report the warning only when it
matters to the user or the changed rule's expected coverage.

If `nudge check` reports "Checked 0 files", the rules parsed but no file-content
rules applied to the provided paths. That can be correct for docs-only or
command-only rule changes.

If a rule fails unexpectedly, prefer tightening the matcher or message over
teaching agents to ignore the warning.
