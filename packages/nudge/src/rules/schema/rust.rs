//! Rust-specific semantic matchers.

use tree_sitter::Node;

use crate::{
    snippet::{Match, Span},
    template::Captures,
};

use super::Language;

#[derive(Debug)]
struct StateCheck {
    receiver: String,
    check_method: String,
    state: CheckedState,
}

#[derive(Debug)]
enum CheckedState {
    None,
    Err,
}

#[derive(Debug)]
struct UnwrapLet {
    receiver: String,
    binding: String,
}

pub(crate) fn check_then_unwrap_matches(source: &str) -> Vec<Match> {
    let Some(tree) = Language::Rust.parse(source) else {
        return Vec::new();
    };

    let mut matches = Vec::new();
    collect_block_matches(tree.root_node(), source, &mut matches);
    matches
}

fn collect_block_matches(node: Node<'_>, source: &str, matches: &mut Vec<Match>) {
    if node.kind() == "block" {
        matches.extend(block_check_then_unwrap_matches(node, source));
    }

    for child in named_children(node) {
        collect_block_matches(child, source, matches);
    }
}

fn block_check_then_unwrap_matches(block: Node<'_>, source: &str) -> Vec<Match> {
    let children = executable_or_commentless_children(block);

    children
        .windows(2)
        .filter_map(|window| check_then_unwrap_match(window[0], window[1], source))
        .collect()
}

fn check_then_unwrap_match(
    guard_statement: Node<'_>,
    unwrap_statement: Node<'_>,
    source: &str,
) -> Option<Match> {
    let if_expression = as_if_expression(guard_statement)?;
    if if_expression.child_by_field_name("alternative").is_some() {
        return None;
    }

    let condition = if_expression.child_by_field_name("condition")?;
    let check = state_check(condition, source)?;

    let consequence = if_expression.child_by_field_name("consequence")?;
    let early_exit = top_level_early_exit(consequence)?;

    let unwrap = unwrap_let(unwrap_statement, source)?;
    if check.receiver != unwrap.receiver {
        return None;
    }

    let span = Span {
        start: guard_statement.byte_range().start,
        end: unwrap_statement.byte_range().end,
    };
    let captures = Captures::from_iter([
        ("receiver".to_string(), check.receiver),
        ("check_method".to_string(), check.check_method),
        (
            "checked_state".to_string(),
            check.state.capture_value().to_string(),
        ),
        ("unwrap_binding".to_string(), unwrap.binding),
        ("early_exit".to_string(), early_exit.to_string()),
        (
            "guard".to_string(),
            node_text(guard_statement, source).to_string(),
        ),
        (
            "unwrap".to_string(),
            node_text(unwrap_statement, source).to_string(),
        ),
    ]);

    Some(Match { span, captures })
}

fn executable_or_commentless_children(node: Node<'_>) -> Vec<Node<'_>> {
    named_children(node)
        .filter(|child| !is_comment(*child))
        .collect()
}

fn named_children(node: Node<'_>) -> impl DoubleEndedIterator<Item = Node<'_>> {
    (0..node.named_child_count()).filter_map(move |index| node.named_child(index))
}

fn is_comment(node: Node<'_>) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

fn as_if_expression(statement: Node<'_>) -> Option<Node<'_>> {
    match statement.kind() {
        "if_expression" => Some(statement),
        "expression_statement" => {
            first_named_child(statement).filter(|child| child.kind() == "if_expression")
        }
        _ => None,
    }
}

fn first_named_child(node: Node<'_>) -> Option<Node<'_>> {
    named_children(node).next()
}

fn state_check(condition: Node<'_>, source: &str) -> Option<StateCheck> {
    let condition = peel_parentheses(condition);

    if condition.kind() == "unary_expression" && first_child_kind(condition) == Some("!") {
        let call = first_named_child(condition).map(peel_parentheses)?;
        let method_call = method_call(call, source)?;
        return match method_call.method.as_str() {
            "is_some" => Some(StateCheck {
                receiver: method_call.receiver,
                check_method: method_call.method,
                state: CheckedState::None,
            }),
            "is_ok" => Some(StateCheck {
                receiver: method_call.receiver,
                check_method: method_call.method,
                state: CheckedState::Err,
            }),
            _ => None,
        };
    }

    let method_call = method_call(condition, source)?;
    match method_call.method.as_str() {
        "is_none" => Some(StateCheck {
            receiver: method_call.receiver,
            check_method: method_call.method,
            state: CheckedState::None,
        }),
        "is_err" => Some(StateCheck {
            receiver: method_call.receiver,
            check_method: method_call.method,
            state: CheckedState::Err,
        }),
        _ => None,
    }
}

fn first_child_kind(node: Node<'_>) -> Option<&str> {
    node.child(0).map(|child| child.kind())
}

fn peel_parentheses(node: Node<'_>) -> Node<'_> {
    if node.kind() == "parenthesized_expression" {
        first_named_child(node)
            .map(peel_parentheses)
            .unwrap_or(node)
    } else {
        node
    }
}

struct MethodCall {
    receiver: String,
    method: String,
}

fn method_call(call: Node<'_>, source: &str) -> Option<MethodCall> {
    if call.kind() != "call_expression" {
        return None;
    }

    let arguments = call.child_by_field_name("arguments")?;
    if arguments.named_child_count() != 0 {
        return None;
    }

    let function = call.child_by_field_name("function")?;
    if function.kind() != "field_expression" {
        return None;
    }

    let receiver = function.child_by_field_name("value")?;
    let method = function.child_by_field_name("field")?;

    Some(MethodCall {
        receiver: node_text(receiver, source).to_string(),
        method: node_text(method, source).to_string(),
    })
}

fn top_level_early_exit(block: Node<'_>) -> Option<&'static str> {
    if block.kind() != "block" {
        return None;
    }

    let last_statement = named_children(block)
        .rev()
        .find(|child| !is_comment(*child))?;
    let expression = if last_statement.kind() == "expression_statement" {
        first_named_child(last_statement)?
    } else {
        last_statement
    };

    match expression.kind() {
        "return_expression" => Some("return"),
        "break_expression" => Some("break"),
        "continue_expression" => Some("continue"),
        _ => None,
    }
}

fn unwrap_let(statement: Node<'_>, source: &str) -> Option<UnwrapLet> {
    if statement.kind() != "let_declaration" {
        return None;
    }

    let binding = statement.child_by_field_name("pattern")?;
    let value = statement.child_by_field_name("value")?;
    let method_call = method_call(peel_parentheses(value), source)?;
    if method_call.method != "unwrap" {
        return None;
    }

    Some(UnwrapLet {
        receiver: method_call.receiver,
        binding: node_text(binding, source).to_string(),
    })
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

impl CheckedState {
    fn capture_value(&self) -> &'static str {
        match self {
            CheckedState::None => "none",
            CheckedState::Err => "err",
        }
    }
}
