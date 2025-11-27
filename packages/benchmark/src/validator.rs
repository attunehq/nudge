//! Validation for matcher results.
//!
//! Validators operate on labeled captures from matchers, checking conditions
//! like existence, containment, or equality of text between captures.

use std::ops::Range;

use bon::Builder;
use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;
use serde::{Deserialize, Serialize};

use crate::matcher::{LabeledCapture, MatchString, Matcher};

/// Validation to apply to matcher results.
///
/// Validators specify conditions that SHOULD be true. If the condition fails,
/// the match is considered a violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Validator {
    /// The match existing is sufficient: validation always passes.
    /// Use when you want to assert that the pattern exists.
    Exists,

    /// The match existing is a problem: validation always fails.
    /// Use when you want to assert the pattern does NOT exist.
    NotExists,

    /// The text between captures should contain the pattern.
    /// If it does not, the match is considered a violation.
    Contains(BetweenCaptures),

    /// The text between captures should NOT contain the pattern.
    /// If it does, the match is considered a violation.
    NotContains(BetweenCaptures),

    /// The text between captures should equal the pattern exactly.
    /// If it does not, the match is considered a violation.
    Equals(BetweenCaptures),
}

/// Parameters for validating text between two captures.
#[derive(Debug, Clone, Serialize, Deserialize, Builder)]
#[non_exhaustive]
pub struct BetweenCaptures {
    /// The name of the first capture, which starts the validation range.
    /// The validated text starts at the END of this capture.
    #[builder(into)]
    pub from: String,

    /// The name of the second capture, which ends the validation range.
    /// The validated text ends at the START of this capture.
    #[builder(into)]
    pub to: String,

    /// The pattern to validate against the text between captures.
    ///
    /// Supports regular expressions. The pattern is checked according to
    /// the validation type (contains, not_contains, equals).
    #[builder(into)]
    pub pattern: MatchString,
}

/// Result of a failed validation.
#[derive(Debug, Clone)]
pub struct Failure {
    /// Description of what was expected.
    pub expected: String,

    /// The actual text that was found.
    pub actual: String,

    /// The byte range of the text that was checked.
    pub span: Range<usize>,
}

impl Validator {
    /// Validate a single set of captures.
    ///
    /// Returns `Ok(None)` if validation passes.
    /// Returns `Ok(Some(failure))` if validation fails.
    /// Returns `Err` if the captures are malformed (missing labels, etc).
    pub fn validate(&self, source: &str, captures: &[LabeledCapture]) -> Result<Option<Failure>> {
        match self {
            Validator::Exists => Ok(None),

            Validator::NotExists => {
                let Some(first) = captures.first() else {
                    return Ok(None);
                };
                Ok(Some(Failure {
                    expected: "pattern should not exist".to_string(),
                    actual: "pattern found".to_string(),
                    span: first.span.clone(),
                }))
            }

            Validator::Contains(bc) => {
                let (between, span) = extract(source, captures, &bc.from, &bc.to)?;
                if bc.pattern.is_match(between) {
                    Ok(None)
                } else {
                    Ok(Some(Failure {
                        expected: format!("contains {:?}", bc.pattern.as_str()),
                        actual: between.to_string(),
                        span,
                    }))
                }
            }

            Validator::NotContains(bc) => {
                let (between, span) = extract(source, captures, &bc.from, &bc.to)?;
                if !bc.pattern.is_match(between) {
                    Ok(None)
                } else {
                    Ok(Some(Failure {
                        expected: format!("does not contain {:?}", bc.pattern.as_str()),
                        actual: between.to_string(),
                        span,
                    }))
                }
            }

            Validator::Equals(bc) => {
                let (between, span) = extract(source, captures, &bc.from, &bc.to)?;
                if bc.pattern.is_exact_match(between) {
                    Ok(None)
                } else {
                    Ok(Some(Failure {
                        expected: format!("equals {:?}", bc.pattern.as_str()),
                        actual: between.to_string(),
                        span,
                    }))
                }
            }
        }
    }
}

/// Extract text between two named captures.
fn extract<'a>(
    source: &'a str,
    captures: &[LabeledCapture],
    from: &str,
    to: &str,
) -> Result<(&'a str, Range<usize>)> {
    let from_capture = captures
        .iter()
        .find(|c| c.label == from)
        .ok_or_else(|| eyre!("capture not found: {from:?}"))?;

    let to_capture = captures
        .iter()
        .find(|c| c.label == to)
        .ok_or_else(|| eyre!("capture not found: {to:?}"))?;

    let start = from_capture.span.end;
    let end = to_capture.span.start;

    if start > end {
        bail!("'from' capture {from:?} ends after 'to' capture {to:?} starts");
    }

    Ok((&source[start..end], start..end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exists_always_passes() {
        let validator = Validator::Exists;
        let captures = vec![LabeledCapture::new("foo", 0..5)];
        assert!(validator.validate("hello", &captures).unwrap().is_none());
    }

    #[test]
    fn test_not_exists_always_fails() {
        let validator = Validator::NotExists;
        let captures = vec![LabeledCapture::new("foo", 0..5)];
        assert!(validator.validate("hello", &captures).unwrap().is_some());
    }

    #[test]
    fn test_not_exists_no_captures_passes() {
        let validator = Validator::NotExists;
        let captures = vec![];
        assert!(validator.validate("hello", &captures).unwrap().is_none());
    }

    #[test]
    fn test_contains_between_captures() {
        let validator = Validator::Contains(
            BetweenCaptures::builder()
                .from("a")
                .to("b")
                .pattern(MatchString::new("\n\n").unwrap())
                .build(),
        );

        let source = "hello\n\nworld";
        let captures = vec![
            LabeledCapture::new("a", 0..5),  // "hello"
            LabeledCapture::new("b", 7..12), // "world"
        ];

        // Between "hello" and "world" is "\n\n" - should pass
        assert!(validator.validate(source, &captures).unwrap().is_none());
    }

    #[test]
    fn test_contains_fails_when_missing() {
        let validator = Validator::Contains(
            BetweenCaptures::builder()
                .from("a")
                .to("b")
                .pattern(MatchString::new("\n\n").unwrap())
                .build(),
        );

        let source = "hello\nworld";
        let captures = vec![
            LabeledCapture::new("a", 0..5),  // "hello"
            LabeledCapture::new("b", 6..11), // "world"
        ];

        // Between "hello" and "world" is "\n" - should fail
        let failure = validator.validate(source, &captures).unwrap();
        assert!(failure.is_some());
    }
}
