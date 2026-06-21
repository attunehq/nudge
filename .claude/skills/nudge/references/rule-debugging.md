# Nudge Rule Debugging

Use this when a Nudge rule is too noisy, does not fire, blocks a surprising
operation, or behaves differently between Claude Code, Codex CLI, and `nudge
check`.

## First Triage

1. Read the exact Nudge message, rule name, file path, command, URL, or prompt
   that triggered the behavior.
2. Identify whether the rule came from user-level `rules.yaml`, `.nudge.yaml`,
   `.nudge.yml`, or `.nudge/**/*.yaml`.
3. Run `nudge validate` to confirm the loaded config and provider support
   warnings.
4. Choose the smallest reproduction path:
   - `nudge test` for one rule and one sample input
   - `nudge check <path>` for file-content rules
   - a saved provider hook payload for provider-specific behavior

## If A Rule Is Too Noisy

Look for broad matchers:

- missing or overly broad `file` glob
- raw `Content` target when the rule should inspect Markdown code blocks only
- regex matching comments, strings, or prose
- missing `project_state` condition for command rules
- message wording that implies a narrower policy than the matcher enforces

Preferred fixes:

1. Narrow `file` before making the content matcher more complex.
2. Use `MarkdownCodeBlock` for rules about examples inside docs.
3. Use `SyntaxTree` when regex is matching comments or strings.
4. Split one ambiguous rule into two direct rules when the fix differs.
5. Update the message so it names the real policy and next step.

## If A Rule Does Not Fire

Check the surface first:

- `Write` rules inspect full new file content through `content`.
- `Edit` rules inspect replacement content through `new_content`.
- Codex file edits are adapted from `apply_patch`; write rules in terms of
  `Write` and `Edit`, not `apply_patch`.
- `nudge check` evaluates file-content rules only.
- Bash, WebFetch, and UserPromptSubmit need live hook payloads.

Common causes:

- `content` used under `Edit` instead of `new_content`
- `new_content` used under `Write` instead of `content`
- glob does not match the file path as seen by the hook
- regex anchoring assumes a single-line input
- TreeSitter query uses the wrong grammar node name
- provider does not expose that hook surface

Use `nudge syntaxtree --language <language> <file-or-source>` when debugging
TreeSitter node names.

## If Behavior Differs By Provider

Read [hook-responses.md](hook-responses.md), then verify whether the provider
exposes the surface:

- Claude Code supports Write, Edit, WebFetch, Bash, and UserPromptSubmit.
- Codex supports UserPromptSubmit and file edits through `apply_patch`
  normalization. Bash coverage depends on the hook event. WebSearch/WebFetch is
  not currently intercepted by Codex hooks.

If provider support is missing, do not work around the rule by pretending it is
universal. Either document the limitation, add a CI-friendly file rule where
possible, or change the rule to a supported surface.

## Debugging Commands

```bash
nudge validate
nudge test
nudge check <path>
nudge syntaxtree --language rust src/lib.rs
```

For command, URL, prompt, or provider-specific behavior, capture the smallest
hook JSON payload that reproduces the issue and run the relevant hook command
against it.

## Fix Checklist

Before calling a rule fixed:

1. The false positive or false negative is reproduced.
2. The rule matcher, target, file glob, or message is changed at the root cause.
3. `nudge validate` passes.
4. `nudge test`, `nudge check`, or the hook payload now proves the intended
   behavior.
5. Any provider limitation is mentioned plainly.
