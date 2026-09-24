use std::ops::Range;

use serde_json::{Value, json};
use tree_sitter::Node;

use crate::{rules::Language, snippet::Span};

#[derive(Debug, Clone)]
pub struct Candidate {
    pub span: Span,
    pub dependency: Range<usize>,
    pub state: Value,
}

pub fn comments(source: &str) -> Result<Vec<Candidate>, &'static str> {
    let tree = Language::Rust
        .parse(source)
        .ok_or("Rust parser did not produce a tree")?;
    if tree.root_node().has_error() {
        return Err("Rust syntax is incomplete; comment context could not be established");
    }
    let mut result = Vec::new();
    visit(tree.root_node(), source, &mut result)?;
    Ok(result)
}

fn ordinary_comment(node: Node<'_>, source: &str) -> bool {
    if !matches!(node.kind(), "line_comment" | "block_comment") {
        return false;
    }
    let text = &source[node.byte_range()];
    !(text.starts_with("//!")
        || text.starts_with("/*!")
        || (text.starts_with("///") && !text.starts_with("////"))
        || (text.starts_with("/**") && !text.starts_with("/***")))
}

fn visit(node: Node<'_>, source: &str, result: &mut Vec<Candidate>) -> Result<(), &'static str> {
    let mut cursor = node.walk();
    let children = node.named_children(&mut cursor).collect::<Vec<_>>();
    let mut i = 0;
    while i < children.len() {
        let first = children[i];
        if !ordinary_comment(first, source) {
            if !matches!(first.kind(), "line_comment" | "block_comment") {
                visit(first, source, result)?;
            }
            i += 1;
            continue;
        }
        let mut last = first;
        while i + 1 < children.len()
            && ordinary_comment(children[i + 1], source)
            && source[last.end_byte()..children[i + 1].start_byte()]
                .trim()
                .is_empty()
            && children[i + 1].start_position().row <= last.end_position().row + 1
        {
            i += 1;
            last = children[i];
        }
        let previous = first.prev_named_sibling().filter(|n| {
            n.end_position().row == first.start_position().row
                && !matches!(n.kind(), "line_comment" | "block_comment")
        });
        let adjacent = previous
            .or_else(|| last.next_named_sibling())
            .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
            .ok_or("comment has no resolvable adjacent code")?;
        let mut enclosing = node;
        while !enclosing.kind().ends_with("_item") {
            match enclosing.parent() {
                Some(parent) => enclosing = parent,
                None => {
                    enclosing = adjacent;
                    break;
                }
            }
        }
        result.push(Candidate {
            span: Span {
                start: first.start_byte(),
                end: last.end_byte(),
            },
            dependency: first.start_byte().min(adjacent.start_byte())
                ..last.end_byte().max(adjacent.end_byte()),
            state: json!({
                "comment": &source[first.start_byte()..last.end_byte()],
                "code": &source[adjacent.byte_range()],
                "context": &source[enclosing.byte_range()],
            }),
        });
        i += 1;
    }
    Ok(())
}
