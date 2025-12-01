use std::{
    fmt::{Display, Formatter},
    ops::Range,
};

use color_eyre::Result;

use color_print::cwriteln;
use serde::{Deserialize, Serialize};

pub mod code;

/// Common functionality for matching types against patterns.
pub trait Matcher<T> {
    /// Find all matches in the target.
    fn find(&self, target: T) -> Matches;
}

/// Common functionality for fallibly matching types against patterns.
pub trait FallibleMatcher<T> {
    /// Find all matches in the target.
    fn find(&self, target: T) -> Result<Matches>;
}

/// Blanket implement `FallibleMatcher` for all `Matcher` instances.
impl<T, M: Matcher<T>> FallibleMatcher<T> for M {
    fn find(&self, target: T) -> Result<Matches> {
        Ok(Matcher::find(self, target))
    }
}

/// A single match from a matcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Match {
    /// The overall span of this match.
    pub span: Span,

    /// Labeled captures within this match.
    pub captures: Vec<LabeledSpan>,
}

impl Match {
    /// Create a new instance with span and captures.
    pub fn new(span: impl Into<Span>, captures: impl IntoIterator<Item = LabeledSpan>) -> Self {
        Self {
            span: span.into(),
            captures: captures.into_iter().collect(),
        }
    }

    /// Create a new instance from just a span, no captures.
    ///
    /// This results in an instance whose `captures` field is empty.
    pub fn unlabeled(span: impl Into<Span>) -> Self {
        Self {
            span: span.into(),
            captures: Vec::new(),
        }
    }

    /// Create a new instance from captures.
    ///
    /// The overall span is derived from the union of the captures' spans.
    /// Returns `None` if captures is empty.
    pub fn from_captures(captures: impl IntoIterator<Item = LabeledSpan>) -> Option<Self> {
        let (captures, span) = captures.into_iter().fold(
            (Vec::new(), (0, 0)),
            |(mut captures, (start, end)), capture| {
                let start = start.min(capture.span.start());
                let end = end.max(capture.span.end());
                captures.push(capture);
                (captures, (start, end))
            },
        );
        if captures.is_empty() {
            return None;
        }

        Some(Self {
            span: span.into(),
            captures,
        })
    }
}

/// Results from running a matcher against a target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Matches {
    /// No matches were found.
    ///
    /// Technically this could be expressed as "an empty vec" in either variant,
    /// but this variant exists so that matchers don't have to imply that the
    /// result is labeled or unlabeled when they have no matches.
    None,

    /// The matcher that created this set of matches does not support labels.
    /// Instead, each match is represented by its `Span`.
    Unlabeled(Vec<Span>),

    /// The matcher that created this set of matches supports labels.
    Labeled(Vec<Match>),
}

impl Matches {
    /// Returns true if no matches were found.
    pub fn is_empty(&self) -> bool {
        match self {
            Matches::Unlabeled(spans) => spans.is_empty(),
            Matches::Labeled(matches) => matches.is_empty(),
            Matches::None => true,
        }
    }

    /// Returns the number of matches.
    pub fn len(&self) -> usize {
        match self {
            Matches::Unlabeled(spans) => spans.len(),
            Matches::Labeled(matches) => matches.len(),
            Matches::None => 0,
        }
    }

    /// Iterate over all matches.
    ///
    /// If the matcher that created this set of matches supported labels,
    /// returns an iterator over [`Match`] instances. Otherwise, returns an
    /// iterator over [`Span`] instances that have been upgraded to [`Match`]
    /// instances via [`Match::unlabeled`].
    pub fn iter(&self) -> impl Iterator<Item = Match> + '_ {
        let unlabeled = match self {
            Matches::Unlabeled(spans) => Some(spans.iter().map(|s| Match::unlabeled(s))),
            Matches::Labeled(_) | Matches::None => None,
        };

        let labeled = match self {
            Matches::Labeled(matches) => Some(matches.iter().cloned()),
            Matches::Unlabeled(_) | Matches::None => None,
        };

        unlabeled
            .into_iter()
            .flatten()
            .chain(labeled.into_iter().flatten())
    }

    /// Iterate over just the spans of all matches.
    pub fn spans(&self) -> impl Iterator<Item = &'_ Span> + '_ {
        match self {
            Matches::Unlabeled(spans) => spans.iter().collect::<Vec<_>>(),
            Matches::Labeled(matches) => matches.iter().map(|m| &m.span).collect::<Vec<_>>(),
            Matches::None => Vec::new(),
        }
        .into_iter()
    }

    /// Get the labeled matches, if the matcher that created this set of matches
    /// supported labels. Returns `None` if the matcher that created this set of
    /// matches did not support labels.
    ///
    /// The use of `Option` here is to allow disambiguation between "did not
    /// support labels" and "found no labeled matches".
    pub fn labeled(&self) -> Option<impl Iterator<Item = &Match> + '_> {
        match self {
            Matches::Unlabeled(_) | Matches::None => None,
            Matches::Labeled(matches) => Some(matches.iter()),
        }
    }

    /// Filter labeled matches using a predicate on the captures.
    ///
    /// This is useful for filtering matches based on text between captures.
    /// Unlabeled matches pass through unchanged since they have no captures.
    pub fn filter_labeled(self, predicate: impl Fn(&[LabeledSpan]) -> bool) -> Self {
        match self {
            Matches::None => Matches::None,
            Matches::Unlabeled(spans) => Matches::Unlabeled(spans),
            Matches::Labeled(matches) => {
                let filtered = matches
                    .into_iter()
                    .filter(|m| predicate(&m.captures))
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    Matches::None
                } else {
                    Matches::Labeled(filtered)
                }
            }
        }
    }
}

/// A span from a matcher.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Span(Range<usize>);

impl Span {
    /// Create a new span.
    pub fn new(start: usize, end: usize) -> Self {
        Self(start..end)
    }

    /// Get the start of the span.
    pub fn start(&self) -> usize {
        self.0.start
    }

    /// Get the end of the span.
    pub fn end(&self) -> usize {
        self.0.end
    }

    /// Get the range of the span.
    pub fn range(&self) -> Range<usize> {
        self.0.clone()
    }
}

impl From<Range<usize>> for Span {
    fn from(span: Range<usize>) -> Self {
        Self(span)
    }
}

impl From<(usize, usize)> for Span {
    fn from((start, end): (usize, usize)) -> Self {
        Self(start..end)
    }
}

impl From<&Span> for Span {
    fn from(span: &Span) -> Self {
        span.clone()
    }
}

impl Display for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start(), self.end())
    }
}

/// A labeled span from a matcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabeledSpan {
    /// The name/label of this capture.
    pub label: String,

    /// The byte range in the source text.
    pub span: Span,
}

impl LabeledSpan {
    /// Create a new labeled capture.
    pub fn new(label: impl Into<String>, span: impl Into<Span>) -> Self {
        Self {
            label: label.into(),
            span: span.into(),
        }
    }

    /// Get the start of the span.
    pub fn start(&self) -> usize {
        self.span.start()
    }

    /// Get the end of the span.
    pub fn end(&self) -> usize {
        self.span.end()
    }

    /// Get the range of the span.
    pub fn range(&self) -> Range<usize> {
        self.span.range()
    }
}

impl<S: Into<String>, R: Into<Span>> From<(S, R)> for LabeledSpan {
    fn from((label, span): (S, R)) -> Self {
        Self::new(label, span)
    }
}

impl From<&LabeledSpan> for LabeledSpan {
    fn from(span: &LabeledSpan) -> Self {
        span.clone()
    }
}

impl Display for LabeledSpan {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let label = &self.label;
        let span = &self.span;
        cwriteln!(f, "<yellow>labeled span:</>")?;
        cwriteln!(f, "<cyan>-</> <yellow>label:</> {label}")?;
        cwriteln!(f, "<cyan>-</> <yellow>span:</> <dim>{span}</>")?;
        Ok(())
    }
}
