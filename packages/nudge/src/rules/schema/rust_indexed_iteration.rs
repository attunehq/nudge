use tree_sitter::Node;

use crate::{snippet::Match, template::Captures};

use super::Language;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Collection {
    display: String,
    normalized: String,
}

pub(super) fn matches(source: &str) -> Vec<Match> {
    let Some(tree) = Language::Rust.parse(source) else {
        return Vec::new();
    };

    let mut matches = Vec::new();
    visit(tree.root_node(), source, &mut matches);
    matches
}

fn visit(node: Node<'_>, source: &str, matches: &mut Vec<Match>) {
    match node.kind() {
        "for_expression" => collect_for_expression_matches(node, source, matches),
        "call_expression" => collect_range_method_matches(node, source, matches),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(child, source, matches);
    }
}

fn collect_for_expression_matches(node: Node<'_>, source: &str, matches: &mut Vec<Match>) {
    let Some(index) = loop_index_name(node, source) else {
        return;
    };
    let Some(range) = node.child_by_field_name("value") else {
        return;
    };
    let Some(collection) = collection_from_zero_len_range(range, source) else {
        return;
    };
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };

    collect_index_expression_matches(body, source, &collection, &index, matches);
}

fn collect_range_method_matches(node: Node<'_>, source: &str, matches: &mut Vec<Match>) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    if function.kind() != "field_expression" {
        return;
    }

    let Some(method) = field_name(function, source) else {
        return;
    };
    if !is_index_closure_method(&method) {
        return;
    }

    let Some(receiver) = function.child_by_field_name("value") else {
        return;
    };
    let Some(collection) = collection_from_zero_len_range(receiver, source) else {
        return;
    };
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return;
    };
    let Some(closure) = first_closure_argument(arguments) else {
        return;
    };
    let Some(index) = closure_index_parameter_name(closure, source, &method) else {
        return;
    };
    let Some(body) = closure.child_by_field_name("body") else {
        return;
    };

    collect_index_expression_matches(body, source, &collection, &index, matches);
}

fn collect_index_expression_matches(
    node: Node<'_>,
    source: &str,
    collection: &Collection,
    index: &str,
    matches: &mut Vec<Match>,
) {
    if introduces_scope_for(node, source, index) {
        return;
    }

    if node.kind() == "index_expression"
        && let Some(indexed_collection) = indexed_collection(node, source)
        && indexed_collection.normalized == collection.normalized
        && let Some(index_expression) = index_expression(node)
        && normalized_node_text(index_expression, source) == index
    {
        matches.push(match_for_index_expression(node, source, collection, index));
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_index_expression_matches(child, source, collection, index, matches);
    }
}

fn match_for_index_expression(
    node: Node<'_>,
    source: &str,
    collection: &Collection,
    index: &str,
) -> Match {
    let captures = Captures::from_iter([
        ("collection".to_string(), collection.display.clone()),
        ("index".to_string(), index.to_string()),
        (
            "indexed".to_string(),
            node_text(node, source).trim().to_string(),
        ),
    ]);

    Match {
        span: node.byte_range().into(),
        captures,
    }
}

fn loop_index_name(node: Node<'_>, source: &str) -> Option<String> {
    let pattern = node.child_by_field_name("pattern")?;
    pattern_identifier_name(pattern, source)
}

fn collection_from_zero_len_range(node: Node<'_>, source: &str) -> Option<Collection> {
    let range = strip_parenthesized_expression(node);
    if range.kind() != "range_expression" || node_text(range, source).contains("..=") {
        return None;
    }

    let start = range.named_child(0)?;
    if normalized_node_text(start, source) != "0" {
        return None;
    }

    let end = range.named_child(1)?;
    let receiver = len_call_receiver(end, source)?;
    Some(collection_from_node(receiver, source))
}

fn len_call_receiver<'tree>(node: Node<'tree>, source: &str) -> Option<Node<'tree>> {
    if node.kind() != "call_expression" {
        return None;
    }

    let arguments = node.child_by_field_name("arguments")?;
    if arguments.named_child_count() != 0 {
        return None;
    }

    let function = node.child_by_field_name("function")?;
    if function.kind() != "field_expression" || field_name(function, source)? != "len" {
        return None;
    }

    function.child_by_field_name("value")
}

fn field_name(node: Node<'_>, source: &str) -> Option<String> {
    let field = node.child_by_field_name("field")?;
    Some(node_text(field, source).trim().to_string())
}

fn indexed_collection(node: Node<'_>, source: &str) -> Option<Collection> {
    let collection = node.named_child(0)?;
    Some(collection_from_node(collection, source))
}

fn index_expression(node: Node<'_>) -> Option<Node<'_>> {
    node.named_child(1)
}

fn collection_from_node(node: Node<'_>, source: &str) -> Collection {
    let node = strip_parenthesized_expression(node);
    let display = node_text(node, source).trim().to_string();
    let normalized = normalize_expression(&display);
    Collection {
        display,
        normalized,
    }
}

fn strip_parenthesized_expression(mut node: Node<'_>) -> Node<'_> {
    while node.kind() == "parenthesized_expression" {
        let Some(child) = node.named_child(0) else {
            break;
        };
        node = child;
    }
    node
}

fn normalized_node_text(node: Node<'_>, source: &str) -> String {
    normalize_expression(node_text(strip_parenthesized_expression(node), source))
}

fn normalize_expression(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect()
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

fn is_index_closure_method(method: &str) -> bool {
    matches!(
        method,
        "all"
            | "any"
            | "filter"
            | "filter_map"
            | "find"
            | "flat_map"
            | "fold"
            | "for_each"
            | "inspect"
            | "map"
            | "position"
            | "rfold"
            | "try_fold"
            | "try_rfold"
    )
}

fn first_closure_argument(arguments: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = arguments.walk();
    arguments
        .named_children(&mut cursor)
        .find(|child| child.kind() == "closure_expression")
}

fn closure_index_parameter_name(closure: Node<'_>, source: &str, method: &str) -> Option<String> {
    let parameter_index = match method {
        "fold" | "try_fold" | "rfold" | "try_rfold" => 1,
        _ => 0,
    };

    let parameters = closure.child_by_field_name("parameters")?;
    closure_parameter_names(parameters, source)
        .into_iter()
        .nth(parameter_index)
}

fn closure_parameter_names(parameters: Node<'_>, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = parameters.walk();

    for child in parameters.named_children(&mut cursor) {
        if let Some(name) = pattern_identifier_name(child, source) {
            names.push(name);
        }
    }

    names
}

fn introduces_scope_for(node: Node<'_>, source: &str, index: &str) -> bool {
    match node.kind() {
        "macro_invocation" | "function_item" => true,
        "closure_expression" => node
            .child_by_field_name("parameters")
            .map(|parameters| {
                closure_parameter_names(parameters, source)
                    .into_iter()
                    .any(|name| name == index)
            })
            .unwrap_or(false),
        "for_expression" => node
            .child_by_field_name("pattern")
            .and_then(|pattern| pattern_identifier_name(pattern, source))
            .is_some_and(|name| name == index),
        _ => false,
    }
}

fn pattern_identifier_name(node: Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node_text(node, source).trim().to_string()),
        "parameter" => node
            .child_by_field_name("pattern")
            .and_then(|pattern| pattern_identifier_name(pattern, source)),
        "mut_pattern" | "ref_pattern" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find_map(|child| pattern_identifier_name(child, source))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::*;

    #[test]
    fn matches_for_loop_range_indexing() {
        let matches = matches(
            r#"
fn process_items(items: &[String]) {
    for i in 0..items.len() {
        let item = &items[i];
        process(i, item);
    }
}
"#,
        );

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(
            matches[0].captures.get("collection"),
            Some(&"items".to_string())
        );
        pretty_assert_eq!(matches[0].captures.get("index"), Some(&"i".to_string()));
        pretty_assert_eq!(
            matches[0].captures.get("indexed"),
            Some(&"items[i]".to_string())
        );
    }

    #[test]
    fn matches_range_map_indexing() {
        let matches = matches(
            r#"
fn clone_items(items: &[String]) -> Vec<String> {
    (0..items.len()).map(|i| items[i].clone()).collect()
}
"#,
        );

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(
            matches[0].captures.get("collection"),
            Some(&"items".to_string())
        );
    }

    #[test]
    fn matches_fold_index_parameter() {
        let matches = matches(
            r#"
fn total_len(items: &[String]) -> usize {
    (0..items.len()).fold(0, |total, i| total + items[i].len())
}
"#,
        );

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(matches[0].captures.get("index"), Some(&"i".to_string()));
    }

    #[test]
    fn ignores_macro_token_trees() {
        let matches = matches(
            r#"
fn assert_items(items: &[String], expected: &[String]) {
    for i in 0..items.len() {
        assert_eq!(items[i], expected[i]);
    }
}
"#,
        );

        assert!(matches.is_empty());
    }

    #[test]
    fn ignores_unrelated_indexing() {
        let matches = matches(
            r#"
fn first_arg(args: &[String]) -> Option<&String> {
    args.get(0).or_else(|| Some(&args[0]))
}
"#,
        );

        assert!(matches.is_empty());
    }

    #[test]
    fn ignores_other_collection_inside_loop() {
        let matches = matches(
            r#"
fn compare(items: &[String], expected: &[String]) {
    for i in 0..items.len() {
        let expected_item = &expected[i];
        process(expected_item);
    }
}
"#,
        );

        assert!(matches.is_empty());
    }

    #[test]
    fn matches_field_receiver_collection() {
        let matches = matches(
            r#"
fn process(self_: &Thing) {
    for i in 0..self_.items.len() {
        process(&self_.items[i]);
    }
}
"#,
        );

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(
            matches[0].captures.get("collection"),
            Some(&"self_.items".to_string())
        );
    }
}
