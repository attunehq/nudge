//! Mermaid grammar support for the tree-sitter parsing library.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_mermaid() -> *const ();
}

/// The tree-sitter language function for Mermaid.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_mermaid) };

/// The content of the `node-types.json` file for this grammar.
pub const NODE_TYPES: &str = include_str!("../../upstream/src/node-types.json");

#[cfg(test)]
mod tests {
    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading Mermaid parser");
    }
}
