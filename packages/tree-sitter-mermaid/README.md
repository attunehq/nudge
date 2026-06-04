# tree-sitter-mermaid

This local crate wraps the generated parser from the
`packages/tree-sitter-mermaid/upstream` Git submodule for Nudge's current
`tree-sitter` API. The submodule points at
`https://github.com/monaqa/tree-sitter-mermaid`.

The upstream repository includes Rust bindings, but they target an older
`tree-sitter` crate API. This wrapper exposes the same `LANGUAGE` constant shape
used by Nudge's other grammar crates through `tree-sitter-language`.
