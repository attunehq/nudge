# RFC 0003: Immediate semantic lint rules

Status: warning-only first slice implemented on 2026-09-19. Research date: 2026-09-19.
The [public guide](../semantic-rules.md) describes shipped behavior. Semantic
blocking, quality qualification, additional selectors, and prompt-intent rules
remain future work.

## Recommendation

Add opt-in natural-language predicates to Nudge's existing file rules, with Jev
as the evaluator. Preserve deterministic matching for facts that code can
establish. Evaluate small code units during the existing PreToolUse hook, before
an edit runs, and return Nudge's authored message at a source location that Nudge
selected. Start with warnings; enable blocking for individual rules after measuring
their false-positive rate and latency.

Use a focused Jev client and keep evaluation policy independently testable.
Local models, provider selection, and a multi-provider abstraction are outside
the scope of this design.

The first vertical slice is [issue #4](https://github.com/attunehq/nudge/issues/4):
comments that merely restate adjacent code. [Issue #6](https://github.com/attunehq/nudge/issues/6)
is a later consumer of the same evaluation layer for prompt-intent matching.
[Issue #8](https://github.com/attunehq/nudge/issues/8), autonomous completion checking,
requires a separate workflow design and is outside this proposal.

A [live feasibility probe](../development/semantic-lint/README.md) is included:
Jev agreed with 11/12 synthetic labels at a 0.5 cutoff and reported 129 ms evaluation
time for the batch. This does not establish production quality or full-hook
latency. The adversarial case made Jev uncertain, reinforcing the need to
distinguish uncertain judgments from clean checks.

## Research findings

### Blink

[Blink](https://blink.review/) describes per-edit code review, mostly powered by
Jev, that sends findings back to the coding agent. Its [quick start](https://blink.review/docs)
claims a review takes less than half a second and offers Claude Code and Codex
hooks. This is a useful interaction reference, not a measured Nudge latency result.
The public material does not establish its prompts, context selection, thresholds,
or exact hook timing; this proposal does not assume those internals.

For Nudge, take the short edit-feedback loop and actionable agent feedback.
Keep conventions explicit and versioned in the repository. A GitHub app, remote
repository index, and autonomous reviewer are unnecessary for the first slice.

### Jev

The [HTTP API](https://docs.typesafe.ai/api) accepts `state`, `model`, and a map of
typed `questions` at `POST https://api.typesafe.ai/v1/systemone`, using bearer
authentication. Independent questions sharing state run together. Use a
[Noul](https://docs.typesafe.ai/primitives/noul) for each violation predicate:
its `noul` field is the probability of yes. It has no separate confidence field.
Jev does not generate a rationale, a fix, or a source span.

Pin `jev-1.13.0` when evaluating thresholds. The [model documentation](https://docs.typesafe.ai/models)
currently specifies $0.042 per million input tokens, free output, a 64k-token
total request limit, and a 32k limit for state plus the longest question. These
are service limits, not appropriate target sizes for immediate linting. For
illustration, 10,000 checks billed at 2,000 input tokens each cost $0.84 at that
rate; actual billing depends on measured token usage and batching.

The vendor's [known limitations](https://docs.typesafe.ai/model-jaggedness/jev-1.13)
include indirection, distracting context, and adversarial state. Typed output
does not prove a judgment correct. Keep compiler/type-system checks deterministic,
and do not use this feature as an authorization boundary. The
[guardrail cookbook](https://docs.typesafe.ai/cookbooks/llm_guardrails) illustrates
separating model scores from application decisions.

## User-facing rule shape

Supported warning-only syntax:

```yaml
version: 1
rules:
  - name: comments-explain-intent
    action: warn
    message: >-
      This comment repeats the code. Remove it or explain the non-obvious
      intent or constraint. Do not invent a reason.
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        semantic:
          select:
            kind: Comments
            language: rust
          violates: >-
            The selected comment merely restates what the adjacent code does
            and adds no useful intent, constraint, contract, or non-obvious context.
          allow: >-
            API documentation, safety invariants, compatibility notes, and
            explanations of a complex algorithm are useful context.
          thresholds:
            clear: 0.1
            violation: 0.9
```

An Edit matcher uses the same semantic block. The thresholds above are candidate
evaluation settings, not established production values. `action: warn` is required.
Semantic `action: block` is rejected until quality qualification is complete.

`semantic` is a sibling to existing content matchers. Existing file, target,
Regex, SyntaxTree, and project-state selectors remain deterministic preconditions.
If present, all preconditions must match before semantic evaluation. Selection
produces candidate units inside that target. For the first slice, implement
`Comments` for Rust; document unsupported language/selector pairs as validation
errors. A later generic AST selector can expand the surface without changing
the evaluator contract. Do not imply all natural-language rules are supported
before their context requirements can be represented.

The Comments selector groups consecutive ordinary comments and provides their
adjacent code plus the enclosing item as context. Recognize doc comments and
exclude them by default. Preserve physical UTF-8 byte spans for annotations.
No candidate means no model call. A comment without resolvable associated code
is an uninspected candidate, not automatically redundant or clean.

Read `TYPESAFE_API_KEY` from the process environment; keep credentials outside
shared rule YAML. Use the fixed TypeSafe API endpoint so a checked-in rule cannot
redirect credentials. Enabling semantic evaluation explicitly permits transmitting
the selected source context to TypeSafe. Missing credentials produce an incomplete
check, not a clean result.

## Evaluation pipeline

1. Normalize the provider operation into a candidate file snapshot and changed ranges.
2. Run deterministic rules and preconditions; return deterministic blocks without a model call.
3. Select relevant candidate units and record their physical spans and context dependencies.
4. Group applicable predicates by identical bounded state.
5. Evaluate each group within the remaining hook deadline.
6. Validate the response and apply the rule's score policy.
7. Render findings using the selected spans and authored messages.

For edits, inspect the proposed resulting file, then evaluate units whose comment
or supporting code intersects the changed ranges. This catches a code change that
makes an unchanged comment redundant and avoids unrelated legacy violations.
Writes inspect every selected unit. `nudge check` inspects every eligible unit.
Deletion-only changes can affect surviving adjacent comments and must participate
in changed-range mapping. Markdown code-block offsets must compose with the
existing target-to-file mapping.

Before this implementation, adapters did not supply a uniform edit contract. Codex apply_patch
normalization puts the entire resulting file in `new_string`; other providers
usually put only replacement text there and optionally reconstruct
`post_edit_content`. Claude's reconstruction currently replaces the first match.
Semantic evaluation now carries a separate exact snapshot with changed byte ranges,
including unambiguous replace-all and sequential multi-edit operations. It reports
incomplete inspection when reconstruction fails. Existing deterministic single-edit
matcher behavior remains unchanged. MultiEdit is normalized as one final snapshot.

### Jev client boundary

Use a focused `JevClient` with an injectable transport for offline tests:

```text
evaluate(state, predicates, deadline)
  -> per-predicate Score | EvaluationError

Score:
  predicate_id, probability, resolved_model

EvaluationError:
  unavailable | timeout | invalid_response | unsupported | input_too_large
```

Nudge owns `Candidate` (source span, selected text, relevant context, snapshot
identity), `Predicate` (violation and allowance definitions), and policy.
The Jev client owns wire formatting and validates Noul probabilities. A model
version change requires revalidating tuned thresholds. Never represent operational
errors as probability zero.

The Jev client builds one Noul per predicate over a shared state. Include the
full meaning in instructions because question IDs are not model instructions.
Responses must contain exactly the requested IDs, the expected answer types,
finite scores in range, and model provenance. Missing or malformed answers make
the affected evaluation incomplete. Do not average unrelated violations.

### Latency and failure behavior

Proposed experience target: p95 additional hook latency below 500 ms for the
agreed small-edit workload, measured from hook entry to rendered response.
Start experiments with a 1,000 ms total semantic deadline, including queueing,
connection setup and decoding. These are acceptance targets, not current results.
Use no automatic retry in the interactive path. Bound multi-file work by the
same total deadline, not a fresh timeout per file.

Scores at or below `clear` are non-findings; scores at or above `violation` are
findings; the middle is uncertain. Uncertain evaluations allow the edit with a
concise warning. Operational failures also allow with an explicit incomplete-check
warning. Blocking applies only to completed, qualified positive judgments.
CI must distinguish complete/clean, violations, and incomplete checks; an
incomplete semantic scan must not print the existing all-clear success message.
Propose exit codes 0, 1, and 2 respectively, with incomplete taking precedence.

Use the existing model-visible warning response where supported, and document
provider delivery limits. Preserve substitution output when a warning is also
present: the current outcome ordering returns warnings before an updated command,
so blindly adding warnings could discard a deterministic substitution. The
implementation now combines those outcomes and regression-tests that behavior.

Batch predicates for the same state first. Measure before adding a daemon for
the remote path or a persistent cache. If caching becomes justified, key raw
scores by exact state, predicate definitions, selector revision, and
pinned model; changing thresholds can reuse scores, changing evidence cannot.
Never cache unavailable results as clean.

## Repository implementation points

- `rules/schema.rs`: semantic config and warning-action validation, keeping
  existing deterministic YAML behavior intact.
- `rules/schema/target.rs`: candidate extraction and physical span mapping.
- `hook.rs` and `agent/*`: explicit snapshot/range coverage and reconstruction errors.
- New `semantic` module: evaluation planning, policy, Jev client, provenance.
- `hook/evaluate.rs`: collect requests across the normalized operation, then render
  outcomes. Do not hide network calls inside `ContentMatcher::is_match()`.
- `cmd/check.rs`: reuse the semantic planner, with complete/incomplete accounting.
- `cmd/test.rs` and `cmd/validate.rs`: fixtures and static configuration validation;
  validation must not require credentials, network access, or model downloads.

The existing External matcher can support an experiment, but is not the product
interface: it discards checker output, treats operational errors as matches,
cannot batch predicates, and does not return semantic spans or score provenance.

## Validation and rollout

First build a labeled comment corpus from issue #4 plus realistic repository
examples. Separate calibration and held-out cases. Include useful contracts,
non-obvious algorithm summaries, misleading use of "because", mixed what/why,
comment-like strings, doc comments, incomplete syntax, Unicode, and adversarial
instructions inside comments. Synthetic smoke tests establish connectivity and
obvious failures, not production precision.

Measure precision, recall, abstention, and coverage per rule/model version. For blocking,
propose at least 99% measured precision on a sufficiently large held-out set,
report the sample count and uncertainty, and explicitly approve the remaining
false-positive tradeoff. A dozen clean examples cannot establish that threshold.
Report cold and warm p50/p95/p99 latency, process startup, request count, input
tokens, and estimated cost.

Contract tests cover deadline expiry, 401/429/5xx, malformed/missing answers,
out-of-range scores, wrong model provenance, oversized input, partial multi-file
coverage, no eligible units, Unicode source spans, and missing credentials. Exercise
actual normalized hook payloads for each supported provider, especially multi-edit
and replace-all operations. Verify that old violations do not block unrelated edits.

Implementation order:

1. Expand the recorded Jev smoke probe into a held-out comment corpus.
2. Add pure schema, selection, and result-policy tests with a fake Jev transport.
3. Add the Jev client and warning-only comment linting in hooks and check mode.
4. Run held-out quality and complete-hook latency evaluations before enabling blocks.
5. Extend selectors and prompt-intent rules after the first slice is useful.

At implementation time, update AGENTS.md, CLAUDE.md, the user/developer guides,
and the bundled Nudge skills for every new supported rule field and outcome.
Do not document this proposal as a shipped feature beforehand.
