use serde::{Deserialize, Deserializer, Serialize};

use crate::snippet::{Match, Span};

use super::content::{RegexMatcher, apply_suggestion, deserialize_regex};

/// The method used to match URLs.
///
/// Similar to [`super::ContentMatcher`] but only supports regex patterns, since
/// URLs are simple strings without syntax tree structure.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum UrlMatcher {
    /// Match on a regular expression.
    Regex {
        /// The regex pattern to match against the URL.
        pattern: RegexMatcher,

        /// Optional suggestion template for this matcher.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        suggestion: Option<String>,
    },
}

impl<'de> Deserialize<'de> for UrlMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            kind: String,
            pattern: Option<String>,
            suggestion: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;

        match raw.kind.as_str() {
            "Regex" => Ok(UrlMatcher::Regex {
                pattern: deserialize_regex(raw.pattern)?,
                suggestion: raw.suggestion,
            }),
            other => Err(serde::de::Error::unknown_variant(other, &["Regex"])),
        }
    }
}

impl UrlMatcher {
    /// Test whether this pattern matches a given URL.
    pub fn is_match(&self, url: &str) -> bool {
        match self {
            UrlMatcher::Regex { pattern, .. } => pattern.is_match(url),
        }
    }

    /// Get the spans of all matches in a given URL.
    pub fn matches(&self, url: &str) -> Vec<Span> {
        match self {
            UrlMatcher::Regex { pattern, .. } => pattern.matches(url),
        }
    }

    /// Get matches with capture groups for template interpolation.
    pub fn matches_with_context(&self, url: &str) -> Vec<Match> {
        match self {
            UrlMatcher::Regex {
                pattern,
                suggestion,
            } => {
                let mut matches = pattern.matches_with_context(url);
                apply_suggestion(&mut matches, suggestion);
                matches
            }
        }
    }
}
