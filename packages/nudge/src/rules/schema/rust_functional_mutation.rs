use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::{
    snippet::{Match, Span},
    template::Captures,
};

use super::syntax::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RustFunctionalMutationPattern {
    VecPush,
    Find,
    Fold,
}

pub fn default_rust_functional_mutation_patterns() -> Vec<RustFunctionalMutationPattern> {
    vec![
        RustFunctionalMutationPattern::VecPush,
        RustFunctionalMutationPattern::Find,
        RustFunctionalMutationPattern::Fold,
    ]
}

pub fn rust_functional_mutation_matches(
    patterns: &[RustFunctionalMutationPattern],
    source: &str,
) -> Vec<Match> {
    let Some(tree) = Language::Rust.parse(source) else {
        return Vec::new();
    };

    let mut matches = Vec::new();
    visit_blocks(tree.root_node(), source, patterns, &mut matches);
    matches
}

fn visit_blocks(
    node: Node<'_>,
    source: &str,
    patterns: &[RustFunctionalMutationPattern],
    matches: &mut Vec<Match>,
) {
    if node.kind() == "block" {
        analyze_block(node, source, patterns, matches);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_blocks(child, source, patterns, matches);
    }
}

fn analyze_block(
    block: Node<'_>,
    source: &str,
    patterns: &[RustFunctionalMutationPattern],
    matches: &mut Vec<Match>,
) {
    let statements = block_statements(block);

    for pair in statements.windows(2) {
        let Some(binding) = mutable_let(pair[0], source) else {
            continue;
        };
        let Some(for_loop) = for_loop(pair[1], source) else {
            continue;
        };

        if has_unsafe_ancestor(binding.0.node) || has_unsafe_ancestor(for_loop.0.node) {
            continue;
        }

        if pattern_enabled(patterns, RustFunctionalMutationPattern::VecPush)
            && let Some(m) = detect_vec_push(&binding, &for_loop, source)
        {
            matches.push(m);
            continue;
        }

        if pattern_enabled(patterns, RustFunctionalMutationPattern::Find)
            && let Some(m) = detect_find(&binding, &for_loop, source)
        {
            matches.push(m);
            continue;
        }

        if pattern_enabled(patterns, RustFunctionalMutationPattern::Fold)
            && let Some(m) = detect_fold(&binding, &for_loop, source)
        {
            matches.push(m);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MutableLet<'tree> {
    node: Node<'tree>,
    init: Node<'tree>,
}

#[derive(Debug, Clone)]
struct Binding {
    name: String,
}

#[derive(Debug, Clone, Copy)]
struct ForLoop<'tree> {
    node: Node<'tree>,
    body: Node<'tree>,
}

#[derive(Debug, Clone)]
struct LoopItem {
    name: String,
    iter: String,
}

#[derive(Debug, Clone, Copy)]
struct PushCall<'tree> {
    argument: Node<'tree>,
}

#[derive(Debug, Clone, Copy)]
struct Assignment<'tree> {
    right: Node<'tree>,
    compound: bool,
}

fn mutable_let<'tree>(node: Node<'tree>, source: &str) -> Option<(MutableLet<'tree>, Binding)> {
    if node.kind() != "let_declaration" || !has_direct_child_kind(node, "mutable_specifier") {
        return None;
    }

    let pattern = node.child_by_field_name("pattern")?;
    if pattern.kind() != "identifier" {
        return None;
    }

    let init = node.child_by_field_name("value")?;
    let name = node_text(pattern, source).to_string();
    Some((MutableLet { node, init }, Binding { name }))
}

fn for_loop<'tree>(node: Node<'tree>, source: &str) -> Option<(ForLoop<'tree>, LoopItem)> {
    let node = unwrap_expression_statement(node);
    if node.kind() != "for_expression" {
        return None;
    }

    let pattern = node.child_by_field_name("pattern")?;
    if pattern.kind() != "identifier" {
        return None;
    }

    let value = node.child_by_field_name("value")?;
    let body = node.child_by_field_name("body")?;
    Some((
        ForLoop { node, body },
        LoopItem {
            name: node_text(pattern, source).to_string(),
            iter: node_text(value, source).trim().to_string(),
        },
    ))
}

fn detect_vec_push(
    (binding, binding_name): &(MutableLet<'_>, Binding),
    (for_loop, loop_item): &(ForLoop<'_>, LoopItem),
    source: &str,
) -> Option<Match> {
    if !is_empty_vec_init(binding.init, source) {
        return None;
    }

    let statements = block_statements(for_loop.body);
    let [statement] = statements.as_slice() else {
        return None;
    };

    if let Some(push) = push_call(*statement, &binding_name.name, source) {
        if !node_contains_identifier(push.argument, &loop_item.name, source)
            || contains_awkward_control_flow(push.argument)
        {
            return None;
        }

        return Some(build_match(
            binding.node,
            for_loop.node,
            [
                ("kind", "vec_push".to_string()),
                ("binding", binding_name.name.clone()),
                ("item", loop_item.name.clone()),
                ("iterator", loop_item.iter.clone()),
                (
                    "suggestion",
                    vec_push_suggestion(binding_name, loop_item, push, source),
                ),
            ],
        ));
    }

    let if_expression = unwrap_expression_statement(*statement);
    if if_expression.kind() != "if_expression"
        || if_expression.child_by_field_name("alternative").is_some()
    {
        return None;
    }

    let condition = if_expression.child_by_field_name("condition")?;
    let consequence = if_expression.child_by_field_name("consequence")?;
    let option_expr = option_expr_from_some_let(condition, source)?;
    if contains_awkward_control_flow(option_expr.value) {
        return None;
    }

    let consequence_statements = block_statements(consequence);
    let [push_statement] = consequence_statements.as_slice() else {
        return None;
    };
    let push = push_call(*push_statement, &binding_name.name, source)?;
    if node_text(push.argument, source).trim() != option_expr.bound_name {
        return None;
    }
    if !node_contains_identifier(option_expr.value, &loop_item.name, source) {
        return None;
    }

    Some(build_match(
        binding.node,
        for_loop.node,
        [
            ("kind", "vec_filter_map".to_string()),
            ("binding", binding_name.name.clone()),
            ("item", loop_item.name.clone()),
            ("iterator", loop_item.iter.clone()),
            (
                "suggestion",
                filter_map_suggestion(binding_name, loop_item, option_expr.value, source),
            ),
        ],
    ))
}

fn detect_find(
    (binding, binding_name): &(MutableLet<'_>, Binding),
    (for_loop, loop_item): &(ForLoop<'_>, LoopItem),
    source: &str,
) -> Option<Match> {
    if !is_none_init(binding.init, source) {
        return None;
    }

    let statements = block_statements(for_loop.body);
    let [statement] = statements.as_slice() else {
        return None;
    };

    let if_expression = unwrap_expression_statement(*statement);
    if if_expression.kind() != "if_expression"
        || if_expression.child_by_field_name("alternative").is_some()
    {
        return None;
    }

    let condition = if_expression.child_by_field_name("condition")?;
    let consequence = if_expression.child_by_field_name("consequence")?;
    let consequence_statements = block_statements(consequence);
    let [assignment_statement, break_statement] = consequence_statements.as_slice() else {
        return None;
    };

    let assignment = assignment(*assignment_statement, &binding_name.name, source)?;
    if assignment.compound {
        return None;
    }

    let some_arg = some_call_arg(assignment.right, source)?;
    if !is_plain_break(*break_statement, source) {
        return None;
    }
    if !node_contains_identifier(condition, &loop_item.name, source)
        && !node_contains_identifier(some_arg, &loop_item.name, source)
    {
        return None;
    }

    let kind = if node_text(some_arg, source).trim() == loop_item.name {
        "find"
    } else {
        "find_map"
    };

    Some(build_match(
        binding.node,
        for_loop.node,
        [
            ("kind", kind.to_string()),
            ("binding", binding_name.name.clone()),
            ("item", loop_item.name.clone()),
            ("iterator", loop_item.iter.clone()),
            (
                "suggestion",
                find_suggestion(binding_name, loop_item, some_arg, source),
            ),
        ],
    ))
}

fn detect_fold(
    (binding, binding_name): &(MutableLet<'_>, Binding),
    (for_loop, loop_item): &(ForLoop<'_>, LoopItem),
    source: &str,
) -> Option<Match> {
    let statements = block_statements(for_loop.body);
    let [statement] = statements.as_slice() else {
        return None;
    };

    let assignment = assignment(*statement, &binding_name.name, source)?;
    if contains_awkward_control_flow(assignment.right)
        || node_contains_kind(assignment.right, "macro_invocation")
    {
        return None;
    }

    let carries_accumulator = assignment.compound
        || node_contains_identifier(assignment.right, &binding_name.name, source);
    if !carries_accumulator || !node_contains_identifier(assignment.right, &loop_item.name, source)
    {
        return None;
    }

    Some(build_match(
        binding.node,
        for_loop.node,
        [
            ("kind", "fold".to_string()),
            ("binding", binding_name.name.clone()),
            ("item", loop_item.name.clone()),
            ("iterator", loop_item.iter.clone()),
            ("init", node_text(binding.init, source).trim().to_string()),
            (
                "suggestion",
                fold_suggestion(binding, binding_name, loop_item, source),
            ),
        ],
    ))
}

#[derive(Debug, Clone, Copy)]
struct SomeLet<'tree> {
    bound_name: &'tree str,
    value: Node<'tree>,
}

fn option_expr_from_some_let<'tree>(
    condition: Node<'tree>,
    source: &'tree str,
) -> Option<SomeLet<'tree>> {
    if condition.kind() != "let_condition" {
        return None;
    }

    let pattern = condition.child_by_field_name("pattern")?;
    let value = condition.child_by_field_name("value")?;
    if pattern.kind() != "tuple_struct_pattern" {
        return None;
    }

    let type_node = pattern.child_by_field_name("type")?;
    if node_text(type_node, source) != "Some" {
        return None;
    }

    let identifiers = named_children(pattern)
        .into_iter()
        .filter(|child| child.kind() == "identifier")
        .filter(|child| node_text(*child, source) != "Some")
        .collect::<Vec<_>>();
    let [bound] = identifiers.as_slice() else {
        return None;
    };

    Some(SomeLet {
        bound_name: node_text(*bound, source),
        value,
    })
}

fn push_call<'tree>(
    statement: Node<'tree>,
    binding_name: &str,
    source: &str,
) -> Option<PushCall<'tree>> {
    let call = unwrap_expression_statement(statement);
    if call.kind() != "call_expression" {
        return None;
    }

    let function = call.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }

    let receiver = function.child_by_field_name("value")?;
    let field = function.child_by_field_name("field")?;
    if receiver.kind() != "identifier"
        || node_text(receiver, source) != binding_name
        || node_text(field, source) != "push"
    {
        return None;
    }

    let arguments = call.child_by_field_name("arguments")?;
    let args = named_children(arguments);
    let [argument] = args.as_slice() else {
        return None;
    };

    Some(PushCall {
        argument: *argument,
    })
}

fn assignment<'tree>(
    statement: Node<'tree>,
    binding_name: &str,
    source: &str,
) -> Option<Assignment<'tree>> {
    let expression = unwrap_expression_statement(statement);
    if expression.kind() != "assignment_expression"
        && expression.kind() != "compound_assignment_expr"
    {
        return None;
    }

    let left = expression.child_by_field_name("left")?;
    if left.kind() != "identifier" || node_text(left, source) != binding_name {
        return None;
    }

    let right = expression.child_by_field_name("right")?;
    Some(Assignment {
        right,
        compound: expression.kind() == "compound_assignment_expr",
    })
}

fn some_call_arg<'tree>(node: Node<'tree>, source: &str) -> Option<Node<'tree>> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function = node.child_by_field_name("function")?;
    if function.kind() != "identifier" || node_text(function, source) != "Some" {
        return None;
    }

    let arguments = node.child_by_field_name("arguments")?;
    let args = named_children(arguments);
    let [arg] = args.as_slice() else {
        return None;
    };

    Some(*arg)
}

fn is_plain_break(statement: Node<'_>, source: &str) -> bool {
    let expression = unwrap_expression_statement(statement);
    expression.kind() == "break_expression" && node_text(expression, source).trim() == "break"
}

fn is_empty_vec_init(node: Node<'_>, source: &str) -> bool {
    if node.kind() == "macro_invocation" {
        let macro_name = node.child_by_field_name("macro");
        let token_tree = named_children(node)
            .into_iter()
            .find(|child| child.kind() == "token_tree");
        return macro_name
            .map(|macro_name| node_text(macro_name, source) == "vec")
            .unwrap_or(false)
            && token_tree
                .map(|token_tree| node_text(token_tree, source).trim() == "[]")
                .unwrap_or(false);
    }

    if node.kind() != "call_expression" {
        return false;
    }

    let Some(arguments) = node.child_by_field_name("arguments") else {
        return false;
    };
    if !named_children(arguments).is_empty() {
        return false;
    }

    let Some(function) = node.child_by_field_name("function") else {
        return false;
    };
    if function.kind() != "scoped_identifier" {
        return false;
    }

    let Some(name) = function.child_by_field_name("name") else {
        return false;
    };
    if node_text(name, source) != "new" {
        return false;
    }

    let Some(path) = function.child_by_field_name("path") else {
        return false;
    };
    type_path_ends_with_vec(path, source)
}

fn type_path_ends_with_vec(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "identifier" | "type_identifier" => node_text(node, source) == "Vec",
        "generic_type" => node
            .child_by_field_name("type")
            .map(|node| type_path_ends_with_vec(node, source))
            .unwrap_or(false),
        "scoped_identifier" | "scoped_type_identifier" => node
            .child_by_field_name("name")
            .map(|node| type_path_ends_with_vec(node, source))
            .unwrap_or(false),
        _ => false,
    }
}

fn is_none_init(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "identifier" => node_text(node, source) == "None",
        "scoped_identifier" => node
            .child_by_field_name("name")
            .map(|name| node_text(name, source) == "None")
            .unwrap_or(false),
        _ => false,
    }
}

fn block_statements(block: Node<'_>) -> Vec<Node<'_>> {
    named_children(block)
        .into_iter()
        .filter(|node| !matches!(node.kind(), "line_comment" | "block_comment"))
        .collect()
}

fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn unwrap_expression_statement(node: Node<'_>) -> Node<'_> {
    if node.kind() != "expression_statement" {
        return node;
    }

    let children = named_children(node);
    let [child] = children.as_slice() else {
        return node;
    };
    *child
}

fn has_direct_child_kind(node: Node<'_>, kind: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|child| child.kind() == kind)
}

fn has_unsafe_ancestor(node: Node<'_>) -> bool {
    let mut current = Some(node);
    while let Some(node) = current {
        if node.kind() == "unsafe_block" {
            return true;
        }
        current = node.parent();
    }
    false
}

fn contains_awkward_control_flow(node: Node<'_>) -> bool {
    [
        "await_expression",
        "break_expression",
        "continue_expression",
        "return_expression",
        "try_expression",
        "unsafe_block",
    ]
    .into_iter()
    .any(|kind| node_contains_kind(node, kind))
}

fn node_contains_kind(node: Node<'_>, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }

    named_children(node)
        .into_iter()
        .any(|child| node_contains_kind(child, kind))
}

fn node_contains_identifier(node: Node<'_>, name: &str, source: &str) -> bool {
    if node.kind() == "identifier" && node_text(node, source) == name {
        return true;
    }

    named_children(node)
        .into_iter()
        .any(|child| node_contains_identifier(child, name, source))
}

fn pattern_enabled(
    patterns: &[RustFunctionalMutationPattern],
    pattern: RustFunctionalMutationPattern,
) -> bool {
    patterns.contains(&pattern)
}

fn build_match(
    start_node: Node<'_>,
    end_node: Node<'_>,
    captures: impl IntoIterator<Item = (&'static str, String)>,
) -> Match {
    Match {
        span: Span::from(start_node.start_byte()..end_node.end_byte()),
        captures: Captures::from_iter(
            captures
                .into_iter()
                .map(|(key, value)| (key.to_string(), value)),
        ),
    }
}

fn vec_push_suggestion(
    binding: &Binding,
    loop_item: &LoopItem,
    push: PushCall<'_>,
    source: &str,
) -> String {
    let iter = iterator_chain(&loop_item.iter);
    let argument = node_text(push.argument, source).trim();
    let replacement = if argument == loop_item.name {
        format!("let {} = {}.collect::<Vec<_>>();", binding.name, iter)
    } else {
        format!(
            "let {} = {}.map(|{}| {}).collect::<Vec<_>>();",
            binding.name, iter, loop_item.name, argument
        )
    };

    format!(
        "Build `{}` with iterator adapters instead of `let mut` plus `push`: `{}`",
        binding.name, replacement
    )
}

fn filter_map_suggestion(
    binding: &Binding,
    loop_item: &LoopItem,
    option_expr: Node<'_>,
    source: &str,
) -> String {
    let iter = iterator_chain(&loop_item.iter);
    let option_expr = node_text(option_expr, source).trim();
    let replacement = format!(
        "let {} = {}.filter_map(|{}| {}).collect::<Vec<_>>();",
        binding.name, iter, loop_item.name, option_expr
    );

    format!(
        "Use `filter_map` and collect directly instead of mutating `{}`: `{}`",
        binding.name, replacement
    )
}

fn find_suggestion(
    binding: &Binding,
    loop_item: &LoopItem,
    some_arg: Node<'_>,
    source: &str,
) -> String {
    let iter = iterator_chain(&loop_item.iter);
    let arg = node_text(some_arg, source).trim();
    if arg == loop_item.name {
        format!(
            "Use `{iter}.find(|{}| ...)` to produce `{}` without `mut` and `break`.",
            loop_item.name, binding.name
        )
    } else {
        format!(
            "Use `{iter}.find_map(|{}| ...)` to produce `{}` without `mut` and `break`.",
            loop_item.name, binding.name
        )
    }
}

fn fold_suggestion(
    binding: &MutableLet<'_>,
    binding_name: &Binding,
    loop_item: &LoopItem,
    source: &str,
) -> String {
    let iter = iterator_chain(&loop_item.iter);
    let init = node_text(binding.init, source).trim();
    format!(
        "Use `{iter}.fold({init}, |{}, {}| ...)` instead of reassigning `{}` inside the loop.",
        binding_name.name, loop_item.name, binding_name.name
    )
}

fn iterator_chain(iterable: &str) -> String {
    let iterable = iterable.trim();
    if iterable.ends_with(".iter()")
        || iterable.ends_with(".iter_mut()")
        || iterable.ends_with(".into_iter()")
    {
        iterable.to_string()
    } else {
        format!("{iterable}.into_iter()")
    }
}

fn node_text<'source>(node: Node<'_>, source: &'source str) -> &'source str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::*;

    #[test]
    fn detects_all_supported_patterns() {
        let source = r#"
fn f(items: Vec<Item>) -> Vec<Value> {
    let mut values = Vec::new();
    for item in items {
        values.push(process(item));
    }

    let mut found = None;
    for item in more_items {
        if item.ready() {
            found = Some(item);
            break;
        }
    }

    let mut total = 0;
    for value in values {
        total = combine(total, value);
    }

    values
}
"#;

        let matches =
            rust_functional_mutation_matches(&default_rust_functional_mutation_patterns(), source);

        pretty_assert_eq!(matches.len(), 3);
        pretty_assert_eq!(
            matches[0].captures.get("kind"),
            Some(&"vec_push".to_string())
        );
        pretty_assert_eq!(matches[1].captures.get("kind"), Some(&"find".to_string()));
        pretty_assert_eq!(matches[2].captures.get("kind"), Some(&"fold".to_string()));
    }

    #[test]
    fn skips_side_effectful_collection_loop() {
        let source = r#"
fn f(items: Vec<Item>) -> Vec<Value> {
    let mut values = Vec::new();
    for item in items {
        metrics.count_item();
        values.push(process(item));
    }
    values
}
"#;

        let matches =
            rust_functional_mutation_matches(&default_rust_functional_mutation_patterns(), source);

        assert!(matches.is_empty());
    }
}
