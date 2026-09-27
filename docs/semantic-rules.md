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
missing source files, or invalid Rust syntax skip the file with a warning.
Consecutive comments are grouped; doc comments and comment-like strings are
excluded. A comment is judged with the code after it, the code before it when it
closes a block, or its enclosing block when that block has no other code.
Comments with no code in scope are not evaluated. Rust fences in Markdown are
supported with `target: { kind: MarkdownCodeBlock, language: rust }`.

Optional `content` (Write) or `new_content` (Edit) matchers act as deterministic
preconditions on each selected target. With semantic Edit rules these conditions
apply to the resulting file or code block, not just the replacement string.

Choose `action: warn` for warning-level findings or `action: block` for
error-level findings that prevent the hook operation. Values at or below `clear`
pass; values at or above `violation` trigger the chosen action. Values between
them are uncertain and reported as warnings, even for `action: block`.

`thresholds.violation` is the probability that the violation statement is true,
not a separate confidence score. For example, `violation: 0.95` triggers at 95%
or higher. The thresholds must satisfy `0 <= clear < violation <= 1`. Choose
thresholds for each rule based on your code and tolerance for false positives.
Semantic judgments are probabilistic; selecting `block` does not make them facts.

Semantic checks are best effort. The client pins `jev-1.13.0`, batches
independent judgments, and sends up to eight batches concurrently. It retries
rate limits (429), server errors including overload (5xx), and connection
failures up to three times with jittered exponential backoff (about 200, 400,
and 800 ms), never past the deadline. It does not retry other failures or follow
HTTP redirects. Hooks have a one-second semantic deadline; `nudge check` has 30
seconds for the whole scan.

When a check cannot run, Nudge reports a skipped-check warning naming the file,
line, and rule it did not evaluate, and why: for example, missing credentials,
exhausted retries, timeout, oversized context, or a malformed answer. Failures
are isolated. A failed request skips only its own batch, and a malformed answer
skips only its own comment. Skipped checks never block a hook or fail CI; they
mean Nudge cannot vouch for that code, not that it is clean.

Source selection and Jev's network call are separate from deterministic checks;
existing block rules still take priority in hooks.

`nudge check` exits 1 for error-level semantic findings or deterministic
violations and 0 otherwise. Warnings, uncertain judgments, and skipped checks
are printed, with a count of skipped checks, but do not fail CI.

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
