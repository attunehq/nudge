use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

use crate::{
    rules::schema::Language,
    snippet::{Match, Span},
    template::Captures,
};

pub fn default_redundant_suffixes() -> Vec<String> {
    ["Manager", "Service", "Handler"]
        .into_iter()
        .map(String::from)
        .collect()
}

pub fn rust_type_name_matches(
    language: Language,
    path: Option<&Path>,
    source: &str,
    redundant_suffixes: &[String],
    module_aliases: &BTreeMap<String, Vec<String>>,
    allow: &[String],
) -> Vec<Match> {
    if language != Language::Rust {
        return Vec::new();
    }

    let Some(tree) = language.parse(source) else {
        return Vec::new();
    };

    let mut module_stack = file_module_stack(path);
    let mut matches = Vec::new();
    visit_node(
        tree.root_node(),
        source,
        &mut module_stack,
        redundant_suffixes,
        module_aliases,
        allow,
        &mut matches,
    );
    matches
}

fn visit_node(
    node: tree_sitter::Node<'_>,
    source: &str,
    module_stack: &mut Vec<String>,
    redundant_suffixes: &[String],
    module_aliases: &BTreeMap<String, Vec<String>>,
    allow: &[String],
    matches: &mut Vec<Match>,
) {
    if node.kind() == "mod_item"
        && let Some(body) = node.child_by_field_name("body")
        && let Some(name) = node
            .child_by_field_name("name")
            .and_then(|node| node_text(node, source))
    {
        module_stack.push(name);
        visit_children(
            body,
            source,
            module_stack,
            redundant_suffixes,
            module_aliases,
            allow,
            matches,
        );
        module_stack.pop();
        return;
    }

    if is_type_item(node.kind())
        && let Some(type_match) = type_item_match(
            node,
            source,
            module_stack,
            redundant_suffixes,
            module_aliases,
            allow,
        )
    {
        matches.push(type_match);
    }

    visit_children(
        node,
        source,
        module_stack,
        redundant_suffixes,
        module_aliases,
        allow,
        matches,
    );
}

fn visit_children(
    node: tree_sitter::Node<'_>,
    source: &str,
    module_stack: &mut Vec<String>,
    redundant_suffixes: &[String],
    module_aliases: &BTreeMap<String, Vec<String>>,
    allow: &[String],
    matches: &mut Vec<Match>,
) {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        visit_node(
            cursor.node(),
            source,
            module_stack,
            redundant_suffixes,
            module_aliases,
            allow,
            matches,
        );

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn type_item_match(
    node: tree_sitter::Node<'_>,
    source: &str,
    module_stack: &[String],
    redundant_suffixes: &[String],
    module_aliases: &BTreeMap<String, Vec<String>>,
    allow: &[String],
) -> Option<Match> {
    let name_node = node.child_by_field_name("name")?;
    let type_name = node_text(name_node, source)?;
    let module_path = module_stack.join("::");

    if is_allowed(allow, &module_path, &type_name) {
        return None;
    }

    let module_terms = module_terms(module_stack, module_aliases);
    let redundant_terms = redundant_terms(redundant_suffixes);
    let findings = findings_for_type(&type_name, &module_terms, &redundant_terms);
    if findings.is_empty() {
        return None;
    }

    let replacement = replacement_for(&type_name, &module_terms, &redundant_terms);
    let primary = findings
        .iter()
        .find(|finding| finding.source == TermSource::Module)
        .unwrap_or_else(|| findings.first().expect("findings is not empty"));
    let span = name_node.byte_range();
    let captures = Captures::from_iter([
        ("0".to_string(), type_name.clone()),
        ("type".to_string(), type_name),
        ("kind".to_string(), type_kind(node.kind()).to_string()),
        ("module".to_string(), module_path.clone()),
        (
            "module_name".to_string(),
            module_stack.last().cloned().unwrap_or_default(),
        ),
        ("term".to_string(), primary.term.clone()),
        (
            "position".to_string(),
            primary.position.as_str().to_string(),
        ),
        ("replacement".to_string(), replacement),
        ("reason".to_string(), primary.reason(&module_path)),
    ]);

    Some(Match {
        span: Span::from(span),
        captures,
    })
}

fn is_type_item(kind: &str) -> bool {
    matches!(
        kind,
        "struct_item" | "enum_item" | "trait_item" | "type_item" | "union_item"
    )
}

fn type_kind(kind: &str) -> &str {
    match kind {
        "struct_item" => "struct",
        "enum_item" => "enum",
        "trait_item" => "trait",
        "type_item" => "type",
        "union_item" => "union",
        _ => "type",
    }
}

fn node_text(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    node.utf8_text(source.as_bytes()).ok().map(String::from)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Term {
    text: String,
    source: TermSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TermSource {
    Module,
    RedundantSuffix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    term: String,
    source: TermSource,
    position: Position,
}

impl Finding {
    fn reason(&self, module_path: &str) -> String {
        match self.source {
            TermSource::Module => {
                if module_path.is_empty() {
                    format!("the surrounding module already provides `{}`", self.term)
                } else {
                    format!("module `{module_path}` already provides `{}`", self.term)
                }
            }
            TermSource::RedundantSuffix => {
                format!("`{}` is configured as a redundant type suffix", self.term)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    Exact,
    Prefix,
    Suffix,
}

impl Position {
    fn as_str(self) -> &'static str {
        match self {
            Position::Exact => "exact",
            Position::Prefix => "prefix",
            Position::Suffix => "suffix",
        }
    }
}

fn findings_for_type(name: &str, module_terms: &[Term], redundant_terms: &[Term]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for term in module_terms {
        if name == term.text {
            findings.push(Finding {
                term: term.text.clone(),
                source: term.source,
                position: Position::Exact,
            });
        } else if strip_suffix(name, &term.text).is_some() {
            findings.push(Finding {
                term: term.text.clone(),
                source: term.source,
                position: Position::Suffix,
            });
        } else if strip_prefix(name, &term.text).is_some() {
            findings.push(Finding {
                term: term.text.clone(),
                source: term.source,
                position: Position::Prefix,
            });
        }
    }

    for term in redundant_terms {
        if strip_suffix(name, &term.text).is_some() {
            findings.push(Finding {
                term: term.text.clone(),
                source: term.source,
                position: Position::Suffix,
            });
        }
    }

    findings
}

fn replacement_for(name: &str, module_terms: &[Term], redundant_terms: &[Term]) -> String {
    let mut candidate = name.to_string();
    let mut terms = module_terms
        .iter()
        .chain(redundant_terms)
        .map(|term| term.text.as_str())
        .collect::<Vec<_>>();
    terms.sort_by_key(|term| Reverse(term.len()));
    terms.dedup();

    loop {
        let before = candidate.clone();

        for term in &terms {
            if let Some(stripped) = strip_suffix(&candidate, term) {
                candidate = stripped;
                break;
            }
        }

        for term in &terms {
            if let Some(stripped) = strip_prefix(&candidate, term) {
                candidate = stripped;
                break;
            }
        }

        if candidate == before {
            break;
        }
    }

    if candidate == name || module_terms.iter().any(|term| term.text == candidate) {
        "a concrete name".to_string()
    } else {
        candidate
    }
}

fn strip_suffix(name: &str, term: &str) -> Option<String> {
    let prefix = name.strip_suffix(term)?;
    if prefix.is_empty() {
        return None;
    }
    Some(prefix.to_string())
}

fn strip_prefix(name: &str, term: &str) -> Option<String> {
    let suffix = name.strip_prefix(term)?;
    if suffix.is_empty() {
        return None;
    }
    let mut chars = suffix.chars();
    let first = chars.next()?;
    if !first.is_uppercase() {
        return None;
    }
    Some(suffix.to_string())
}

fn module_terms(
    module_stack: &[String],
    module_aliases: &BTreeMap<String, Vec<String>>,
) -> Vec<Term> {
    let mut terms = Vec::new();
    let mut seen = BTreeSet::new();

    for module in module_stack {
        push_term(
            &mut terms,
            &mut seen,
            to_pascal_case(module),
            TermSource::Module,
        );

        if let Some(aliases) = module_aliases.get(module) {
            for alias in aliases {
                push_term(
                    &mut terms,
                    &mut seen,
                    configured_term(alias),
                    TermSource::Module,
                );
            }
        }
    }

    terms.sort_by_key(|term| Reverse(term.text.len()));
    terms
}

fn redundant_terms(suffixes: &[String]) -> Vec<Term> {
    let mut terms = Vec::new();
    let mut seen = BTreeSet::new();

    for suffix in suffixes {
        push_term(
            &mut terms,
            &mut seen,
            configured_term(suffix),
            TermSource::RedundantSuffix,
        );
    }

    terms.sort_by_key(|term| Reverse(term.text.len()));
    terms
}

fn push_term(terms: &mut Vec<Term>, seen: &mut BTreeSet<String>, text: String, source: TermSource) {
    if text.is_empty() || !seen.insert(text.clone()) {
        return;
    }
    terms.push(Term { text, source });
}

fn configured_term(value: &str) -> String {
    if value.chars().any(char::is_uppercase) {
        value.to_string()
    } else {
        to_pascal_case(value)
    }
}

fn to_pascal_case(value: &str) -> String {
    value
        .split(['_', '-', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            first
                .to_uppercase()
                .chain(chars.flat_map(char::to_lowercase))
                .collect::<String>()
        })
        .collect()
}

fn is_allowed(allow: &[String], module_path: &str, type_name: &str) -> bool {
    let qualified = if module_path.is_empty() {
        type_name.to_string()
    } else {
        format!("{module_path}::{type_name}")
    };

    allow.iter().any(|allowed| {
        allowed == type_name
            || allowed == &qualified
            || (allowed.contains("::") && qualified.ends_with(&format!("::{allowed}")))
    })
}

fn file_module_stack(path: Option<&Path>) -> Vec<String> {
    let Some(path) = path else {
        return Vec::new();
    };

    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(String::from),
            _ => None,
        })
        .collect::<Vec<_>>();
    let start = components
        .iter()
        .rposition(|component| component == "src")
        .map_or(0, |index| index + 1);
    let module_components = &components[start..];
    if module_components
        .first()
        .is_some_and(|component| is_non_module_source_root(component))
    {
        return Vec::new();
    }

    module_components
        .iter()
        .enumerate()
        .filter_map(|(index, component)| {
            let is_last = index + 1 == module_components.len();
            let module = if is_last {
                Path::new(component).file_stem()?.to_str()?
            } else {
                component
            };

            if is_skipped_module_name(module) || !is_rust_identifier(module) {
                return None;
            }

            Some(module.to_string())
        })
        .collect()
}

fn is_non_module_source_root(component: &str) -> bool {
    matches!(component, "bin" | "benches" | "examples" | "tests")
}

fn is_skipped_module_name(module: &str) -> bool {
    matches!(module, "" | "." | "lib" | "main" | "mod")
}

fn is_rust_identifier(module: &str) -> bool {
    let mut chars = module.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq as pretty_assert_eq;

    fn matches(path: Option<&Path>, source: &str) -> Vec<Match> {
        rust_type_name_matches(
            Language::Rust,
            path,
            source,
            &default_redundant_suffixes(),
            &BTreeMap::from([("db".to_string(), vec!["Database".to_string()])]),
            &[],
        )
    }

    #[test]
    fn detects_file_module_suffix() {
        let matches = matches(
            Some(Path::new("src/storage.rs")),
            "pub struct CasStorage;\n",
        );

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(matches[0].captures["type"], "CasStorage");
        pretty_assert_eq!(matches[0].captures["module"], "storage");
        pretty_assert_eq!(matches[0].captures["replacement"], "Cas");
    }

    #[test]
    fn detects_inline_module_alias_exact() {
        let matches = matches(None, "mod db { pub struct Database; }");

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(matches[0].captures["type"], "Database");
        pretty_assert_eq!(matches[0].captures["term"], "Database");
        pretty_assert_eq!(matches[0].captures["replacement"], "a concrete name");
    }

    #[test]
    fn allow_list_suppresses_exact_qualified_name() {
        let matches = rust_type_name_matches(
            Language::Rust,
            Some(Path::new("src/storage.rs")),
            "pub struct StorageEngine;\n",
            &default_redundant_suffixes(),
            &BTreeMap::new(),
            &["storage::StorageEngine".to_string()],
        );

        assert!(matches.is_empty());
    }

    #[test]
    fn file_module_stack_uses_path_after_src() {
        pretty_assert_eq!(
            file_module_stack(Some(Path::new(
                "packages/nudge/src/rules/schema/content.rs"
            ))),
            vec!["rules", "schema", "content"]
        );
        pretty_assert_eq!(
            file_module_stack(Some(Path::new("src/storage/mod.rs"))),
            vec!["storage"]
        );
    }

    #[test]
    fn file_module_stack_ignores_non_module_roots() {
        assert!(file_module_stack(Some(Path::new("src/bin/tool.rs"))).is_empty());
        assert!(file_module_stack(Some(Path::new("tests/storage.rs"))).is_empty());
    }
}
