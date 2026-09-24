# Markdown code-block targets must exclude closing fences

## What went wrong

The Jev comment selector rejected valid fenced Rust with a syntax error. In
`rules/schema/target.rs`, the pulldown-cmark Start event range covers the whole
fenced block. Initializing both body_start and body_end to range.end retained the
closing fence in the source passed to matchers. Regex and recovering tree-sitter
queries hid the problem.

## Fix

Initialize body_start to the Start range end and body_end to its start. Let Text
event ranges establish the actual body bounds, preserving physical byte offsets.
Never treat the Start event's range end as the initial body end.

## Verification

`markdown_segments_exclude_fences` checks exact source and offset mapping.
`markdown_spans_and_duplicate_write_edit_rules_are_stable` checks semantic
selection and physical line reporting with the same target implementation.
