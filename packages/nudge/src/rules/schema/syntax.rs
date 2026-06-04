use std::sync::{LazyLock, Mutex, MutexGuard};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tree_sitter::{Language as TsLanguage, Parser, Query, QueryError};

use crate::snippet::Span;

/// Supported languages for tree-sitter parsing.
///
/// Adding a new language requires:
/// 1. Adding the grammar crate to Cargo.toml (e.g., `tree-sitter-python`)
/// 2. Adding a variant to this enum
/// 3. Adding a match arm to `grammar()` that returns the language
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// The Rust programming language.
    Rust,
    /// The TypeScript programming language.
    #[value(name = "typescript", alias = "type-script")]
    TypeScript,
    /// The JavaScript programming language.
    #[value(name = "javascript", alias = "java-script")]
    JavaScript,
    /// The Python programming language.
    Python,
    /// The Go programming language.
    Go,
    /// The Java programming language.
    Java,
    /// The C# programming language.
    #[value(name = "csharp", alias = "c-sharp")]
    CSharp,
    /// The Kotlin programming language.
    Kotlin,
    /// The Haskell programming language.
    Haskell,
    /// The Mermaid diagram syntax.
    Mermaid,
}

impl Language {
    /// Get the tree-sitter grammar for this language.
    pub fn grammar(self) -> TsLanguage {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            Language::Java => tree_sitter_java::LANGUAGE.into(),
            Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Language::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
            Language::Haskell => tree_sitter_haskell::LANGUAGE.into(),
            Language::Mermaid => tree_sitter_mermaid::LANGUAGE.into(),
        }
    }

    /// Parse source code into a syntax tree.
    ///
    /// Returns `None` if parsing fails. We intentionally do not reject parse
    /// trees with syntax errors since code being written is often incomplete.
    pub fn parse(self, source: &str) -> Option<tree_sitter::Tree> {
        let mut parser = lock_parser(parser_for(self));
        let tree = parser.parse(source, None)?;

        if tree.root_node().has_error() {
            tracing::debug!(language = ?self, "parsed code contains syntax errors");
        }

        Some(tree)
    }
}

fn parser_for(language: Language) -> &'static Mutex<Parser> {
    static RUST: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::Rust));
    static TYPESCRIPT: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::TypeScript));
    static JAVASCRIPT: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::JavaScript));
    static PYTHON: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::Python));
    static GO: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::Go));
    static JAVA: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::Java));
    static CSHARP: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::CSharp));
    static KOTLIN: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::Kotlin));
    static HASKELL: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::Haskell));
    static MERMAID: LazyLock<Mutex<Parser>> = LazyLock::new(|| new_parser(Language::Mermaid));

    match language {
        Language::Rust => &RUST,
        Language::TypeScript => &TYPESCRIPT,
        Language::JavaScript => &JAVASCRIPT,
        Language::Python => &PYTHON,
        Language::Go => &GO,
        Language::Java => &JAVA,
        Language::CSharp => &CSHARP,
        Language::Kotlin => &KOTLIN,
        Language::Haskell => &HASKELL,
        Language::Mermaid => &MERMAID,
    }
}

fn new_parser(language: Language) -> Mutex<Parser> {
    let mut parser = Parser::new();
    parser
        .set_language(&language.grammar())
        .unwrap_or_else(|error| panic!("failed to set {language:?} language: {error}"));
    Mutex::new(parser)
}

pub(super) fn lock_parser<T>(parser: &Mutex<T>) -> MutexGuard<'_, T> {
    parser.lock().expect("parser mutex poisoned")
}

/// Compute the union span of all captures in a tree-sitter match.
pub(super) fn union_of_captures(m: &tree_sitter::QueryMatch) -> Span {
    if m.captures.is_empty() {
        tracing::warn!("tree-sitter match has no captures");
        return Span { start: 0, end: 0 };
    }

    let (start, end) = m.captures.iter().fold((usize::MAX, 0), |(start, end), c| {
        let range = c.node.byte_range();
        (start.min(range.start), end.max(range.end))
    });

    if start > end {
        tracing::warn!("tree-sitter match produced invalid byte ranges");
        return Span { start: 0, end: 0 };
    }

    Span { start, end }
}

/// A compiled tree-sitter query.
///
/// Wraps `tree_sitter::Query` with the original source and language for
/// serialization and cloning. Queries are compiled at deserialization time to
/// catch errors early.
#[derive(Debug)]
pub struct TreeSitterQuery {
    inner: Query,
    source: String,
    language: Language,
}

impl TreeSitterQuery {
    /// Compile a query from source for the given language.
    pub fn new(language: Language, source: impl Into<String>) -> Result<Self, QueryError> {
        let source = source.into();
        let inner = Query::new(&language.grammar(), &source)?;
        Ok(Self {
            inner,
            source,
            language,
        })
    }
}

impl AsRef<Query> for TreeSitterQuery {
    fn as_ref(&self) -> &Query {
        &self.inner
    }
}

impl Clone for TreeSitterQuery {
    fn clone(&self) -> Self {
        Self::new(self.language, &self.source).expect("query compiled before, should compile again")
    }
}

impl Serialize for TreeSitterQuery {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.source)
    }
}

impl<'de> Deserialize<'de> for TreeSitterQuery {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Err(serde::de::Error::custom(
            "TreeSitterQuery cannot be deserialized standalone; use ContentMatcher::SyntaxTree",
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{panic::catch_unwind, sync::Mutex};

    use clap::ValueEnum;
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::*;

    #[test]
    fn test_language_parse_valid_rust() {
        let code = "fn main() { println!(\"hello\"); }";
        let tree = Language::Rust.parse(code);
        assert!(tree.is_some());
    }

    #[test]
    fn test_language_parse_valid_java() {
        let code = "class Test { void run() { System.out.println(\"hello\"); } }";
        let tree = Language::Java.parse(code);
        assert!(tree.is_some());
        assert!(
            !tree
                .expect("valid Java should parse")
                .root_node()
                .has_error()
        );
    }

    #[test]
    fn test_language_parse_invalid_returns_tree_with_errors() {
        let code = "fn main( { }";
        let tree = Language::Rust.parse(code);
        assert!(tree.is_some());
    }

    #[test]
    fn test_language_parse_invalid_java_returns_tree_with_errors() {
        let code = "class Test { void run() { System.out.println(\"hello\") ";
        let tree = Language::Java
            .parse(code)
            .expect("incomplete Java should still produce a tree");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_typescript_parse_invalid_returns_tree_with_errors() {
        let code = "function process(data: any";
        let tree = Language::TypeScript
            .parse(code)
            .expect("TypeScript parser should return an error-tolerant tree");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_language_parse_rust_reuses_parser_for_multiple_sources() {
        let first = Language::Rust
            .parse("fn first() {}")
            .expect("parse first source");
        let second = Language::Rust
            .parse("fn second() { let value: usize = 1; }")
            .expect("parse second source");

        pretty_assert_eq!(first.root_node().kind(), "source_file");
        pretty_assert_eq!(second.root_node().kind(), "source_file");
        assert!(!second.root_node().has_error());
    }

    #[test]
    fn test_language_parse_invalid_javascript_returns_tree_with_errors() {
        let code = "if (user == null) {";
        let tree = Language::JavaScript
            .parse(code)
            .expect("JavaScript parser returns trees for incomplete code");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_language_parse_valid_python() {
        let code = "def main():\n    print(\"hello\")\n";
        let tree = Language::Python.parse(code);
        assert!(tree.is_some());
    }

    #[test]
    fn test_language_parse_invalid_python_returns_tree_with_errors() {
        let code = "def main(:\n    print(\"hello\")\n";
        let tree = Language::Python.parse(code);
        assert!(tree.is_some());
        assert!(
            tree.expect("parser should recover a tree")
                .root_node()
                .has_error(),
            "invalid Python should produce a recovered tree with errors"
        );
    }

    #[test]
    fn test_go_parse_invalid_returns_tree_with_errors() {
        let code = "package main\nfunc main( { panic(\"boom\")";
        let tree = Language::Go
            .parse(code)
            .expect("Go parser should recover an error tree");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_cli_language_values_match_yaml_names_with_legacy_aliases() {
        pretty_assert_eq!(
            Language::from_str("typescript", false),
            Ok(Language::TypeScript)
        );
        pretty_assert_eq!(
            Language::from_str("type-script", false),
            Ok(Language::TypeScript)
        );
        pretty_assert_eq!(
            Language::from_str("javascript", false),
            Ok(Language::JavaScript)
        );
        pretty_assert_eq!(
            Language::from_str("java-script", false),
            Ok(Language::JavaScript)
        );
        pretty_assert_eq!(Language::from_str("csharp", false), Ok(Language::CSharp));
        pretty_assert_eq!(Language::from_str("c-sharp", false), Ok(Language::CSharp));
        pretty_assert_eq!(Language::from_str("mermaid", false), Ok(Language::Mermaid));
    }

    #[test]
    fn test_language_parse_mermaid_reuses_parser_and_accepts_error_trees() {
        let valid = "flowchart TD\n  Start --> Done\n";
        let first_tree = Language::Mermaid.parse(valid).expect("parse valid Mermaid");
        let second_tree = Language::Mermaid.parse(valid).expect("parse Mermaid again");

        pretty_assert_eq!(first_tree.root_node().kind(), "source_file");
        pretty_assert_eq!(second_tree.root_node().kind(), "source_file");
        assert!(!first_tree.root_node().has_error());

        let incomplete = "flowchart TD\n  Start -->";
        let tree = Language::Mermaid
            .parse(incomplete)
            .expect("parse incomplete Mermaid into an error tree");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_language_parse_haskell_reuses_parser_and_accepts_error_trees() {
        let valid = "module Main where\n\nfirstElem xs = head xs\n";
        let first_tree = Language::Haskell.parse(valid).expect("parse valid haskell");
        let second_tree = Language::Haskell.parse(valid).expect("parse haskell again");

        pretty_assert_eq!(first_tree.root_node().kind(), "haskell");
        pretty_assert_eq!(second_tree.root_node().kind(), "haskell");

        let incomplete = "module Main where\n\nfirstElem xs =";
        let tree = Language::Haskell
            .parse(incomplete)
            .expect("parse incomplete haskell into an error tree");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_language_parse_invalid_csharp_returns_tree_with_errors() {
        let code = "public class Test { public void Example( { Console.WriteLine(\"debug\"); }";
        let tree = Language::CSharp
            .parse(code)
            .expect("C# parser should return a tree");
        assert!(tree.root_node().has_error());
    }

    #[test]
    fn test_lock_parser_panics_when_mutex_is_poisoned() {
        let parser = Mutex::new(());
        let poison = catch_unwind(|| {
            let _guard = parser.lock().expect("initial parser lock should succeed");
            panic!("poison parser mutex");
        });

        assert!(poison.is_err());
        assert!(parser.is_poisoned());

        let result = catch_unwind(|| {
            let _guard = lock_parser(&parser);
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_treesitter_query_compile_valid() {
        let query = TreeSitterQuery::new(Language::Rust, "(function_item)");
        assert!(query.is_ok());
    }

    #[test]
    fn test_treesitter_query_compile_valid_java() {
        let query = TreeSitterQuery::new(
            Language::Java,
            "(method_invocation name: (identifier) @method)",
        );
        assert!(query.is_ok());
    }

    #[test]
    fn test_treesitter_query_compile_valid_csharp() {
        let query = TreeSitterQuery::new(Language::CSharp, "(method_declaration)");
        assert!(query.is_ok());
    }

    #[test]
    fn test_treesitter_query_compile_invalid() {
        let query = TreeSitterQuery::new(Language::Rust, "(not_a_real_node)");
        assert!(query.is_err());
    }

    #[test]
    fn test_treesitter_query_compile_valid_haskell() {
        let query = TreeSitterQuery::new(
            Language::Haskell,
            r#"
            (apply
              function: (variable) @fn
              (#eq? @fn "head"))
            "#,
        );
        assert!(query.is_ok());
    }

    #[test]
    fn test_treesitter_query_compile_valid_mermaid() {
        let query = TreeSitterQuery::new(Language::Mermaid, "(diagram_flow) @diagram");
        assert!(query.is_ok());
    }

    #[test]
    fn test_treesitter_query_compile_invalid_java() {
        let query = TreeSitterQuery::new(Language::Java, "(not_a_real_java_node)");
        assert!(query.is_err());
    }

    #[test]
    fn test_python_treesitter_query_compile_valid() {
        let query = TreeSitterQuery::new(
            Language::Python,
            "(call function: (identifier) @function_name)",
        );
        assert!(query.is_ok());
    }

    #[test]
    fn test_python_treesitter_query_compile_invalid() {
        let query = TreeSitterQuery::new(Language::Python, "(not_a_real_python_node)");
        assert!(query.is_err());
    }
}
