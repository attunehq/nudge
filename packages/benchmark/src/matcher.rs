use std::ops::Range;

use color_eyre::Result;

pub use string::Match as MatchString;

pub mod code;
pub mod string;

/// A labeled capture from a matcher.
///
/// Represents a named region of text identified by a matcher pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledCapture {
    /// The name/label of this capture.
    pub label: String,

    /// The byte range in the source text.
    pub span: Range<usize>,
}

impl LabeledCapture {
    /// Create a new labeled capture.
    pub fn new(label: impl Into<String>, span: Range<usize>) -> Self {
        Self {
            label: label.into(),
            span,
        }
    }
}

impl<S: Into<String>> From<(S, Range<usize>)> for LabeledCapture {
    fn from((label, span): (S, Range<usize>)) -> Self {
        Self::new(label, span)
    }
}

/// Common functionality for matching types against patterns.
pub trait Matcher<T> {
    /// Check if self matches the target.
    fn is_match(&self, target: T) -> bool;

    /// Check if self equals the target exactly.
    fn is_exact_match(&self, target: T) -> bool;

    /// Find the first match for target in self, returning its range.
    fn find(&self, target: T) -> Option<Range<usize>>;

    /// Find all matches for target in self, returning their ranges.
    fn find_all(&self, target: T) -> impl Iterator<Item = Range<usize>>;

    /// Find all labeled matches for target in self, returning their ranges.
    fn find_all_labeled(&self, target: T) -> impl Iterator<Item = LabeledCapture>;
}

/// Common functionality for fallibly matching types against patterns.
pub trait FallibleMatcher<T> {
    /// Check if self matches the target.
    fn is_match(&self, target: T) -> Result<bool>;

    /// Check if self equals the target exactly.
    fn is_exact_match(&self, target: T) -> Result<bool>;

    /// Find the first match for target in self, returning its range.
    fn find(&self, target: T) -> Result<Option<Range<usize>>>;

    /// Find all matches for target in self, returning their ranges.
    fn find_all(&self, target: T) -> Result<impl Iterator<Item = Range<usize>>>;

    /// Find all labeled matches for target in self, returning their ranges.
    fn find_all_labeled(&self, target: T) -> Result<impl Iterator<Item = LabeledCapture>>;
}

/// Blanket implement `FallibleMatcher` for all `Matcher` instances.
impl<T, M: Matcher<T>> FallibleMatcher<T> for M {
    fn is_match(&self, target: T) -> Result<bool> {
        Ok(Matcher::is_match(self, target))
    }

    fn is_exact_match(&self, target: T) -> Result<bool> {
        Ok(Matcher::is_exact_match(self, target))
    }

    fn find(&self, target: T) -> Result<Option<Range<usize>>> {
        Ok(Matcher::find(self, target))
    }

    fn find_all(&self, target: T) -> Result<impl Iterator<Item = Range<usize>>> {
        Ok(Matcher::find_all(self, target))
    }

    fn find_all_labeled(&self, target: T) -> Result<impl Iterator<Item = LabeledCapture>> {
        Ok(Matcher::find_all_labeled(self, target))
    }
}
