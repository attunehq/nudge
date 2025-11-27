use std::ops::Range;

use bon::Builder;
use color_eyre::{
    Result, Section, SectionExt,
    eyre::{Context, OptionExt, eyre},
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_plain::derive_display_from_serialize;
use tap::Pipe;
use tree_sitter::{
    Language as TsLanguage, Parser as TsParser, Query as TsQuery, QueryCursor as TsQueryCursor,
    StreamingIterator as _, Tree as TsTree,
};

use crate::matcher::{FallibleMatcher, LabeledCapture};

/// Matches target code against a query.
#[derive(Debug, Builder)]
#[non_exhaustive]
pub struct Match {
    /// The language of the code to match.
    pub language: Language,

    /// The query pattern to match.
    pub query: Query,
}

impl<'a> FallibleMatcher<&'a str> for Match {
    fn is_match(&self, target: &str) -> Result<bool> {
        self.find_all(target)?.next().is_some().pipe(Ok)
    }

    fn is_exact_match(&self, target: &'a str) -> Result<bool> {
        self.find_all(target)?
            .fold((0, 0), |(start, end), range| {
                (start.min(range.start), end.max(range.end))
            })
            .pipe(|(start, end)| start == 0 && end == target.len())
            .pipe(Ok)
    }

    fn find(&self, target: &'a str) -> Result<Option<Range<usize>>> {
        let tree = self
            .language
            .parse_code(target)
            .context("parse code for match")?;

        let mut cursor = TsQueryCursor::new();
        let query = self.query.as_ref();

        cursor
            .matches(query, tree.root_node(), target.as_bytes())
            .next()
            .and_then(|m| m.captures.iter().map(|c| c.node.byte_range()).next())
            .pipe(Ok)
    }

    fn find_all(&self, target: &'a str) -> Result<impl Iterator<Item = Range<usize>>> {
        let tree = self
            .language
            .parse_code(target)
            .context("parse code for match")?;

        let mut cursor = TsQueryCursor::new();
        let query = self.query.as_ref();
        let mut matches = cursor.matches(query, tree.root_node(), target.as_bytes());

        let mut captures = Vec::new();
        while let Some(matched) = matches.next() {
            let Some(range) = matched.captures.iter().map(|c| c.node.byte_range()).next() else {
                continue;
            };
            captures.push(range);
        }

        Ok(captures.into_iter())
    }

    fn find_all_labeled(&self, target: &str) -> Result<impl Iterator<Item = LabeledCapture>> {
        let tree = self
            .language
            .parse_code(target)
            .context("parse code for match")?;

        let mut cursor = TsQueryCursor::new();
        let query = self.query.as_ref();
        let mut matches = cursor.matches(query, tree.root_node(), target.as_bytes());
        let capture_names = query.capture_names();

        let mut captures = Vec::new();
        while let Some(matched) = matches.next() {
            let matched = matched
                .captures
                .iter()
                .map(|c| {
                    let index = c.index as usize;
                    let label = capture_names
                        .get(index)
                        .ok_or_else(|| eyre!("capture index is out of bounds: {index}"))
                        .with_section(|| format!("{c:?}").header("Capture:"))
                        .with_section(|| capture_names.join(", ").header("Capture names:"))
                        .with_section(|| format!("{query:?}").header("Query:"))
                        .with_section(|| format!("{target:?}").header("Code:"))?;
                    Ok(LabeledCapture::new(*label, c.node.byte_range()))
                })
                .collect::<Result<Vec<_>>>()
                .context("extract captures from match")?;

            captures.extend(matched);
        }

        captures.sort_by_key(|c| c.span.start);
        Ok(captures.into_iter())
    }
}

impl<'de> Deserialize<'de> for Match {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize to this first so that we can use the language to parse
        // the query, which treesitter requires.
        #[derive(Debug, Deserialize)]
        struct Intermediate {
            language: Language,
            query: String,
        }

        // Then we can parse the query with the language.
        let Intermediate { language, query } = Intermediate::deserialize(deserializer)?;
        let query = TsQuery::new(&language.treesitter(), &query)
            .map(Query)
            .map_err(serde::de::Error::custom)?;
        Ok(Match { language, query })
    }
}

/// The language of the code to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// The Rust programming language.
    Rust,
}

impl Language {
    fn treesitter(self) -> TsLanguage {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }

    fn parse_code(self, code: impl AsRef<str>) -> Result<TsTree> {
        let code = code.as_ref();
        let mut parser = TsParser::new();
        parser
            .set_language(&self.treesitter())
            .context("set language for parser")
            .with_section(|| self.to_string().header("Language:"))?;
        parser
            .parse(code, None)
            .ok_or_eyre("parse code")
            .with_section(|| self.to_string().header("Language:"))
            .with_section(|| code.to_string().header("Code:"))
    }
}

derive_display_from_serialize!(Language);

/// A query pattern used for matching code.
///
/// Uses tree-sitter's S-expression query syntax. Reference:
/// <https://tree-sitter.github.io/tree-sitter/using-parsers/queries/index.html>
#[derive(Debug)]
pub struct Query(TsQuery);

impl Query {
    pub fn parse(language: Language, query: impl AsRef<str>) -> Result<Self> {
        let query = query.as_ref();
        TsQuery::new(&language.treesitter(), query)
            .map(Self)
            .with_context(|| format!("parse query: {query:?}"))
            .with_section(|| query.to_string().header("Query:"))
            .with_section(|| language.to_string().header("Language:"))
    }
}

impl AsRef<TsQuery> for Query {
    fn as_ref(&self) -> &TsQuery {
        &self.0
    }
}
