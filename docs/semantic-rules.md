# Natural-language lint rules with Jev

Semantic rules evaluate ordinary Rust comments in the context of adjacent code.
They run during supported Write/Edit hooks and `nudge check`. Run
`nudge login typesafe.ai` to save a TypeSafe API key, or set `TYPESAFE_API_KEY`
in the environment. Selected source context is sent to TypeSafe; credentials
stay out of rule YAML.
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

Choose `action: warn` for warning-level findings or `action: block` for
error-level findings that prevent the hook operation. Values at or below `clear`
pass; values at or above `violation` trigger the chosen action. Values between
them report uncertainty and allow the operation. API failures and incomplete
checks also allow the operation with a warning, even for `action: block`.

`thresholds.violation` is the probability that the violation statement is true,
not a separate confidence score. For example, `violation: 0.95` triggers at 95%
or higher. The thresholds must satisfy `0 <= clear < violation <= 1`. Choose
thresholds for each rule based on your code and tolerance for false positives.
Semantic judgments are probabilistic; selecting `block` does not make them facts.

The client pins `jev-1.13.0`, batches independent judgments, and uses a one-second
semantic deadline for a hook invocation. It does not retry failures or follow
HTTP redirects. Missing credentials, rate limiting, timeout, oversized context,
or malformed responses produce explicit incomplete warnings. Check mode has a
30-second network-evaluation budget for the scan. Source selection and Jev's
network call are separate from deterministic checks; existing block rules still
take priority in hooks.

`nudge check` exits 0 for a complete scan with no errors, including warning-only
findings; 1 for error-level semantic findings or deterministic violations; and 2
for uncertain or incomplete semantic checks. Incomplete or uncertain takes
precedence over errors, and never prints an all-clear result. Warnings are printed
but do not fail CI.

Validate configuration offline with `nudge validate`. For a sample evaluation:

```sh
nudge test --rule comments-explain-intent --tool Write \
  --file sample.rs --content-file sample.rs
```

Plain `nudge test --tool Edit` supplies a replacement fragment, so semantic
evaluation reports missing full-file context; use a Write sample or an actual
Edit hook to test the judgment.

## Login and credential storage

1. Create an API key at <https://console.typesafe.ai/keys>.
2. Run `nudge login typesafe.ai`.
3. Paste the key into the hidden prompt.

The command verifies the key with a small synthetic Jev request before saving it.
A failed verification leaves any saved key unchanged. For automation, pipe the
key from your secret store into `nudge login typesafe.ai --stdin`; never put it
in command-line arguments or repository files.

Nudge prints the saved path. It uses `credentials.json` in the same user config
directory as global `rules.yaml`:

- macOS: `~/Library/Application Support/com.attunehq.nudge/credentials.json`
- Linux: `$XDG_CONFIG_HOME/nudge/credentials.json`, or `~/.config/nudge/credentials.json`
- Windows: `%APPDATA%\attunehq\nudge\config\credentials.json`

The file stores the key as plaintext, with owner-only permissions on Unix. Each
hook process reads it when needed, so saved credentials work without restarting
the agent. A non-empty `TYPESAFE_API_KEY` overrides the saved credential; use this
for CI secrets or a temporary account override. An agent using an environment
override must inherit it from its launching process.
