# Natural-language lint rules with Jev

Semantic rules evaluate ordinary Rust comments in the context of adjacent code.
They run during supported Write/Edit hooks and `nudge check`. Set
`TYPESAFE_API_KEY` in the environment of the agent or CLI before use. Selected
source context is sent to TypeSafe; credentials stay out of rule YAML.
Repositories without semantic rules make no Jev requests.

```yaml
version: 1
rules:
  - name: comments-explain-intent
    action: warn
    message: >-
      Remove this redundant comment or explain the non-obvious intent or
      constraint. Do not invent a reason.
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        semantic:
          select: { kind: Comments, language: rust }
          violates: >-
            The selected comment merely restates what the adjacent code does
            and adds no useful intent, constraint, contract, or non-obvious context.
          allow: >-
            The comment explains intent, an API contract, a safety invariant,
            compatibility, or non-obvious context.
          thresholds: { clear: 0.1, violation: 0.9 }
```

Add an equivalent `tool: Edit` matcher to check edits. On Edit, Nudge evaluates
comments whose text or adjacent code changed in the resulting file. It does not
interpret a replacement fragment as a complete file. Ambiguous replacements,
missing source files, invalid Rust syntax, or comments without associated code
produce incomplete-check warnings. Consecutive comments are grouped; doc comments
and comment-like strings are excluded. Rust fences in Markdown are supported with
`target: { kind: MarkdownCodeBlock, language: rust }`.

Optional `content` (Write) or `new_content` (Edit) matchers act as deterministic
preconditions on each selected target. With semantic Edit rules these conditions
apply to the resulting file or code block, not just the replacement string.

This first release requires `action: warn` for semantic rules. Values at or below
`clear` pass, values at or above `violation` report a finding, and values between
them report uncertainty. These example thresholds need evaluation on your own
code. Warnings allow the tool operation; they are not verified facts or security
gates. Semantic blocking awaits held-out quality evaluation.

The client pins `jev-1.13.0`, batches independent judgments, and uses a one-second
semantic deadline for a hook invocation. It does not retry failures or follow
HTTP redirects. Missing credentials, rate limiting, timeout, oversized context,
or malformed responses produce explicit incomplete warnings. Check mode has a
30-second network-evaluation budget for the scan. Source selection and Jev's
network call are separate from deterministic checks; existing block rules still
take priority in hooks.

`nudge check` exits 0 for a complete scan with no findings, 1 for findings, and 2
for uncertain or incomplete semantic checks. Incomplete or uncertain takes
precedence over findings, and never prints an all-clear result. Although hooks
only warn, completed semantic findings fail check mode so CI can surface them.

Validate configuration offline with `nudge validate`. For a sample evaluation:

```sh
nudge test --rule comments-explain-intent --tool Write \
  --file sample.rs --content-file sample.rs
```

Plain `nudge test --tool Edit` supplies a replacement fragment, so semantic
evaluation reports missing full-file context; use a Write sample or an actual
Edit hook to test the judgment. Existing agents must inherit the credential from
their launching environment; saving an environment file alone does not update
an already-running agent process.
