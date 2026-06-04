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
    TypeScript,
    /// The JavaScript programming language.
    JavaScript,
    /// The Python programming language.
    Python,
    /// The Go programming language.
    Go,
    /// The Java programming language.
    Java,
    /// The C# programming language.
    CSharp,
    /// The Kotlin programming language.
    Kotlin,
    /// The Haskell programming language.
    Haskell,
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

    use super::*;

    #[test]
    fn test_language_parse_valid_rust() {
        let code = "fn main() { println!(\"hello\"); }";
        let tree = Language::Rust.parse(code);
        assert!(tree.is_some());
    }

    #[test]
    fn test_language_parse_invalid_returns_tree_with_errors() {
        let code = "fn main( { }";
        let tree = Language::Rust.parse(code);
        assert!(tree.is_some());
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
    fn test_treesitter_query_compile_invalid() {
        let query = TreeSitterQuery::new(Language::Rust, "(not_a_real_node)");
        assert!(query.is_err());
    }
}
