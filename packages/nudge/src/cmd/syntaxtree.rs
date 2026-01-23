//! Display the syntax tree for a code snippet.
//!
//! Useful for understanding tree structure when writing tree-sitter queries.
//! Shows node kinds (what you match in queries) and field names.

use std::fs;
use std::path::Path;

use clap::Args;
use color_eyre::Result;
use color_print::cformat;

use nudge::rules::Language;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Language to parse the code as.
    #[arg(short, long)]
    language: Language,

    /// Code to parse, or a path to a file containing the code.
    input: String,
}

pub fn main(config: Config) -> Result<()> {
    let code = resolve_input(&config.input);

    let Some(tree) = config.language.parse(&code) else {
        println!("Failed to parse code");
        return Ok(());
    };

    let nodes = collect_syntax_nodes(tree.root_node(), &code, 0, None);
    for node in nodes {
        println!("{}", node.render());
    }

    Ok(())
}

/// Resolve the input as either a file path or literal code.
fn resolve_input(input: &str) -> String {
    let path = Path::new(input);
    if path.exists() {
        fs::read_to_string(path).unwrap_or_else(|_| input.to_string())
    } else {
        input.to_string()
    }
}

/// A flattened syntax node for rendering.
struct SyntaxNode {
    depth: usize,
    kind: String,
    field_name: Option<String>,
    text: String,
}

impl SyntaxNode {
    fn render(&self) -> String {
        let indent = "  ".repeat(self.depth);
        let field = self
            .field_name
            .as_deref()
            .map(|name| cformat!("<cyan>{name}:</> "))
            .unwrap_or_default();
        let kind = &self.kind;

        // Only show text for small nodes without newlines
        if self.text.len() <= 40 && !self.text.contains('\n') {
            let text = &self.text;
            cformat!("{indent}{field}<green>{kind}</> <dim>{text:?}</>")
        } else {
            cformat!("{indent}{field}<green>{kind}</> <dim>..</>")
        }
    }
}

/// Recursively collect syntax nodes from a tree-sitter node.
fn collect_syntax_nodes(
    node: tree_sitter::Node,
    source: &str,
    depth: usize,
    field_name: Option<&str>,
) -> Vec<SyntaxNode> {
    let mut nodes = Vec::new();

    // Add this node
    let text = node
        .utf8_text(source.as_bytes())
        .unwrap_or_default()
        .to_string();

    nodes.push(SyntaxNode {
        depth,
        kind: node.kind().to_string(),
        field_name: field_name.map(String::from),
        text,
    });

    // Recursively add children
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let child_field = cursor.field_name();
            nodes.extend(collect_syntax_nodes(child, source, depth + 1, child_field));

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    nodes
}
