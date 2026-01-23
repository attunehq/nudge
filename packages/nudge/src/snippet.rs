//! Source code snippet rendering for rule violations.
//!
//! Uses `annotate-snippets` to render compiler-like diagnostic output,
//! pointing to the exact location of matches in source code.

use std::ops::Range;

use annotate_snippets::{Level, Renderer, Snippet};
use bon::Builder;
use derive_more::AsRef;

use crate::template::Captures;

/// Source code to be annotated.
#[derive(Debug, Clone, PartialEq, Eq, AsRef)]
pub struct Source(String);

impl Source {
    /// Annotate the source code with the given annotations.
    ///
    /// # Examples
    ///
    /// Produces output similar to Rust compiler diagnostics:
    ///
    /// ```text
    /// error: Rule violation. Fix this error and immediately retry.
    ///    |
    ///  5 |     let foo = bar.unwrap();
    ///    |               ^^^^^^^^^^^^ Do not use `.unwrap()`.
    ///    |
    /// ```
    pub fn annotate(&self, annotations: impl IntoIterator<Item = impl Into<Annotation>>) -> String {
        // We have to double iterate here because the `annotate_snippets` API
        // requires a `&'a str` for the label; if we just iterate once and don't
        // hang on to the labels as a `Vec` the labels don't live long enough
        // because they're dropped before we can render them.
        let annotations = annotations.into_iter().map(Into::into).collect::<Vec<_>>();
        let annotations = annotations
            .iter()
            .map(|Annotation { span, label }| Level::Error.span(span.range()).label(label));

        let title = if annotations.len() == 1 {
            "Rule violation. Fix this error and immediately retry."
        } else {
            "Rule violations. Fix these errors and immediately retry."
        };

        let snippet = Snippet::source(self.0.as_ref()).annotations(annotations);
        let message = Level::Error.title(title).snippet(snippet);
        Renderer::plain().render(message).to_string()
    }
}

impl<S: Into<String>> From<S> for Source {
    fn from(source: S) -> Self {
        Self(source.into())
    }
}

/// An annotation on a source code snippet.
#[derive(Debug, Clone, PartialEq, Eq, Builder)]
pub struct Annotation {
    /// The byte range of the annotation.
    #[builder(into)]
    pub span: Span,

    /// The label of the annotation.
    #[builder(into, default = Annotation::DEFAULT_LABEL)]
    pub label: String,
}

impl Annotation {
    /// The default label for an annotation if created without a label.
    pub const DEFAULT_LABEL: &str = "matched pattern";
}

impl<S: Into<Span>, L: Into<String>> From<(S, L)> for Annotation {
    fn from((span, label): (S, L)) -> Self {
        Self {
            span: span.into(),
            label: label.into(),
        }
    }
}

impl<S: Into<Span>> From<S> for Annotation {
    fn from(span: S) -> Self {
        Self {
            span: span.into(),
            label: Self::DEFAULT_LABEL.into(),
        }
    }
}

impl From<&Annotation> for Annotation {
    fn from(annotation: &Annotation) -> Self {
        annotation.clone()
    }
}

/// A byte range in source content.
#[derive(Debug, Clone, PartialEq, Eq, Builder)]
pub struct Span {
    /// Start byte offset.
    pub start: usize,

    /// End byte offset.
    pub end: usize,
}

impl Span {
    /// View the span as a `Range<usize>`.
    pub fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

impl From<Range<usize>> for Span {
    fn from(range: Range<usize>) -> Self {
        Self {
            start: range.start,
            end: range.end,
        }
    }
}

impl From<(usize, usize)> for Span {
    fn from((start, end): (usize, usize)) -> Self {
        Self { start, end }
    }
}

/// A match with capture groups for template interpolation.
///
/// Carries both the location (span) and captured data from a regex match,
/// enabling per-match interpolation of suggestions.
#[derive(Debug, Clone, PartialEq, Eq, Builder)]
pub struct Match {
    /// The byte range of the match.
    #[builder(into)]
    pub span: Span,

    /// Captured groups from the regex match.
    ///
    /// Keys are `"0"`, `"1"`, `"2"` for positional captures,
    /// and the capture name for named captures.
    #[builder(default)]
    pub captures: Captures,
}

impl From<Span> for Match {
    fn from(span: Span) -> Self {
        Self {
            span,
            captures: Default::default(),
        }
    }
}

impl From<Range<usize>> for Match {
    fn from(range: Range<usize>) -> Self {
        Self {
            span: Span::from(range),
            captures: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_render_snippet_single_span() {
        let source = "let foo = bar.unwrap();";
        let spans = vec![super::Span::builder().start(10).end(22).build()]; // "bar.unwrap()"
        let result = super::Source::from(source).annotate(spans);

        assert!(result.contains("1"));
        assert!(result.contains("let foo = bar.unwrap();"));
        assert!(result.contains("matched pattern"));
        assert!(result.contains("^"));
    }

    #[test]
    fn test_render_snippet_multiple_spans() {
        let source = "fn main() {\n    use std::io;\n    use std::fs;\n}";
        let use1_start = source
            .find("use std::io")
            .expect("source contains use std::io");
        let use1_end = use1_start + "use std::io".len();
        let use2_start = source
            .find("use std::fs")
            .expect("source contains use std::fs");
        let use2_end = use2_start + "use std::fs".len();

        let result = super::Source::from(source).annotate([
            (use1_start..use1_end, "use std::io"),
            (use2_start..use2_end, "use std::fs"),
        ]);
        assert!(result.contains("use std::io"));
        assert!(result.contains("use std::fs"));
    }
}
