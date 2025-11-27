use std::{
    fmt::{Display, Formatter, Result as FormatterResult},
    ops::Range,
};

use color_eyre::{Result, Section, SectionExt, eyre::Context};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::matcher::{LabeledCapture, Matcher};

/// Matches a target string against a regular expression.
#[derive(Debug, Clone)]
pub struct Match(Regex);

impl Match {
    /// Create a new content matcher from a string.
    /// Attempts to compile as a regex first; falls back to literal string.
    pub fn new(content: impl AsRef<str>) -> Result<Self> {
        let content = content.as_ref();
        Regex::new(&content)
            .map(Self)
            .with_context(|| format!("compile regex: {content:?}"))
            .with_section(|| content.to_string().header("Content:"))
    }

    /// Read the content as a string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'a> Matcher<&'a str> for Match {
    fn is_match(&self, target: &str) -> bool {
        self.0.is_match(target)
    }

    fn is_exact_match(&self, target: &str) -> bool {
        self.0
            .find(target)
            .is_some_and(|m| m.start() == 0 && m.end() == target.len())
    }

    fn find(&self, target: &str) -> Option<Range<usize>> {
        self.0.find(target).map(|m| m.range())
    }

    fn find_all(&self, target: &str) -> impl Iterator<Item = Range<usize>> {
        self.0.find_iter(target).map(|m| m.range())
    }

    fn find_all_labeled(&self, target: &str) -> impl Iterator<Item = LabeledCapture> {
        let labels = self.0.capture_names().flatten().collect::<Vec<_>>();
        let mut captures = Vec::new();

        for capture in self.0.captures_iter(target) {
            for label in labels.iter() {
                let Some(matched) = capture.name(label) else {
                    continue;
                };
                captures.push(LabeledCapture::new(*label, matched.range()));
            }
        }

        captures.sort_by_key(|c| c.span.start);
        captures.into_iter()
    }
}

impl Display for Match {
    fn fmt(&self, f: &mut Formatter<'_>) -> FormatterResult {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for Match {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Match {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let expr = String::deserialize(deserializer)?;
        Match::new(expr).map_err(serde::de::Error::custom)
    }
}
