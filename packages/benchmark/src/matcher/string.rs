use std::fmt::{Display, Formatter, Result as FormatterResult};

use color_eyre::{Result, Section, SectionExt, eyre::Context};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tap::{Conv, Pipe};

use crate::matcher::{LabeledSpan, Match, Matcher, Matches, Span};

/// Matches a target string against a regular expression.
#[derive(Debug, Clone)]
pub struct RegexMatcher(Regex);

impl RegexMatcher {
    /// Create a new regex matcher.
    pub fn new(pattern: impl AsRef<str>) -> Result<Self> {
        let pattern = pattern.as_ref();
        Regex::new(pattern)
            .map(Self)
            .with_context(|| format!("compile regex: {pattern:?}"))
            .with_section(|| pattern.to_string().header("Pattern:"))
    }

    /// Get the pattern as a string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Check if this pattern has any named capture groups.
    fn has_named_captures(&self) -> bool {
        self.0.capture_names().flatten().next().is_some()
    }

    /// Check if the pattern matches anywhere in the target.
    pub fn is_match(&self, target: &str) -> bool {
        self.0.is_match(target)
    }

    /// Check if the pattern matches the entire target exactly.
    pub fn is_exact_match(&self, target: &str) -> bool {
        self.0
            .find(target)
            .is_some_and(|m| m.start() == 0 && m.end() == target.len())
    }
}

impl<'a> Matcher<&'a str> for RegexMatcher {
    fn find(&self, target: &str) -> Matches {
        // Groups can be named or unnamed. In order to support both, we label
        // each capture group by its 1-based index prefixed by `$`, and
        // additionally label named capture groups.
        //
        // For example, the following regex:
        // ```
        // (?<name>\w+)\s*,\s*(\d+)
        // ```
        //
        // Would evaluate to the following labels:
        // - `name` referencing the group `(?<name>\w+)`
        // - `$1` referencing the group `(?<name>\w+)`
        // - `$2` referencing the group `(\d+)`
        let labels = self.0.capture_names().flatten().collect::<Vec<_>>();

        match self.0.captures_len() {
            // Since `captures_len` always includes the implicit group that
            // wraps the entire expression, its result is always guaranteed to
            // be at least 1. But if it's exactly 1, that means the user
            // provided no groups, and therefore there's nothing useful to use
            // to label matches.
            1 => self
                .0
                .find_iter(target)
                .map(|m| m.range().into())
                .collect::<Vec<_>>()
                .pipe(Matches::Unlabeled),
            // This iterates over matches for the overall expression, and then
            // inside of each match there's also an iterator of captures.
            // - Iterate over named labels and try to match them if we can.
            // - Iterate over unlabeled groups and generate a `${idx}` label
            //   for each one, starting at `$0` for the overall match.
            groups => {
                let mut matches = Vec::new();

                for entry in self.0.captures_iter(target) {
                    let span = entry.get_match().range().conv::<Span>();
                    let mut captures = Vec::new();
                    for label in labels.iter().copied() {
                        if let Some(capture) = entry.name(label) {
                            captures.push(LabeledSpan::new(label, capture.range()));
                        }
                    }
                    for i in 0..groups {
                        if let Some(capture) = entry.get(i) {
                            let label = format!("${i}");
                            captures.push(LabeledSpan::new(label, capture.range()));
                        }
                    }
                    matches.push(Match::new(span, captures));
                }

                Matches::Labeled(matches)
            }
        }
    }
}

impl Display for RegexMatcher {
    fn fmt(&self, f: &mut Formatter<'_>) -> FormatterResult {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for RegexMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RegexMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let pattern = String::deserialize(deserializer)?;
        RegexMatcher::new(pattern).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_groups_returns_unlabeled() {
        let matcher = RegexMatcher::new(r"hello").unwrap();
        let matches = matcher.find("hello world hello");

        match matches {
            Matches::Unlabeled(spans) => {
                assert_eq!(spans.len(), 2);
                assert_eq!(spans[0].range(), 0..5);
                assert_eq!(spans[1].range(), 12..17);
            }
            _ => panic!("expected Unlabeled, got {:?}", matches),
        }
    }

    #[test]
    fn test_unnamed_groups_returns_labeled_with_dollar_indices() {
        let matcher = RegexMatcher::new(r"(\w+)\s+(\d+)").unwrap();
        let matches = matcher.find("foo 123");

        match matches {
            Matches::Labeled(ref ms) => {
                assert_eq!(ms.len(), 1);
                let m = &ms[0];

                // Overall span
                assert_eq!(m.span.range(), 0..7);

                // Should have $0 (whole match), $1 (first group), $2 (second group)
                let labels: Vec<_> = m.captures.iter().map(|c| c.label.as_str()).collect();
                assert!(labels.contains(&"$0"), "missing $0, got {:?}", labels);
                assert!(labels.contains(&"$1"), "missing $1, got {:?}", labels);
                assert!(labels.contains(&"$2"), "missing $2, got {:?}", labels);

                // Check spans
                let dollar_1 = m.captures.iter().find(|c| c.label == "$1").unwrap();
                assert_eq!(dollar_1.range(), 0..3); // "foo"

                let dollar_2 = m.captures.iter().find(|c| c.label == "$2").unwrap();
                assert_eq!(dollar_2.range(), 4..7); // "123"
            }
            _ => panic!("expected Labeled, got {:?}", matches),
        }
    }

    #[test]
    fn test_named_groups_returns_labeled_with_names_and_indices() {
        let matcher = RegexMatcher::new(r"(?P<word>\w+)\s+(?P<num>\d+)").unwrap();
        let matches = matcher.find("foo 123");

        match matches {
            Matches::Labeled(ref ms) => {
                assert_eq!(ms.len(), 1);
                let m = &ms[0];

                // Should have named labels AND dollar indices
                let labels: Vec<_> = m.captures.iter().map(|c| c.label.as_str()).collect();
                assert!(labels.contains(&"word"), "missing 'word', got {:?}", labels);
                assert!(labels.contains(&"num"), "missing 'num', got {:?}", labels);
                assert!(labels.contains(&"$0"), "missing $0, got {:?}", labels);
                assert!(labels.contains(&"$1"), "missing $1, got {:?}", labels);
                assert!(labels.contains(&"$2"), "missing $2, got {:?}", labels);

                // Check named captures have correct spans
                let word = m.captures.iter().find(|c| c.label == "word").unwrap();
                assert_eq!(word.range(), 0..3); // "foo"

                let num = m.captures.iter().find(|c| c.label == "num").unwrap();
                assert_eq!(num.range(), 4..7); // "123"
            }
            _ => panic!("expected Labeled, got {:?}", matches),
        }
    }

    #[test]
    fn test_multiple_matches() {
        let matcher = RegexMatcher::new(r"(?P<n>\d+)").unwrap();
        let matches = matcher.find("a1b2c3");

        match matches {
            Matches::Labeled(ref ms) => {
                assert_eq!(ms.len(), 3);

                assert_eq!(ms[0].span.range(), 1..2); // "1"
                assert_eq!(ms[1].span.range(), 3..4); // "2"
                assert_eq!(ms[2].span.range(), 5..6); // "3"

                // Each match should have the 'n' label
                for m in ms {
                    assert!(m.captures.iter().any(|c| c.label == "n"));
                }
            }
            _ => panic!("expected Labeled, got {:?}", matches),
        }
    }

    #[test]
    fn test_no_matches_returns_unlabeled_empty() {
        let matcher = RegexMatcher::new(r"xyz").unwrap();
        let matches = matcher.find("hello world");

        match matches {
            Matches::Unlabeled(spans) => {
                assert!(spans.is_empty());
            }
            _ => panic!("expected Unlabeled, got {:?}", matches),
        }
    }
}
