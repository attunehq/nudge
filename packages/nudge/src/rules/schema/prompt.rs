use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    snippet::{Match, Span},
    template::Captures,
};

use super::GlobMatcher;

/// Duration in seconds, parsed from friendly strings like `30m`, `1h`, or `2d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct DurationSeconds(u64);

impl DurationSeconds {
    pub fn as_secs(self) -> u64 {
        self.0
    }
}

impl Default for DurationSeconds {
    fn default() -> Self {
        Self(60 * 60)
    }
}

impl<'de> Deserialize<'de> for DurationSeconds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Seconds(u64),
            Text(String),
        }

        match Raw::deserialize(deserializer)? {
            Raw::Seconds(seconds) => Ok(DurationSeconds(seconds)),
            Raw::Text(text) => parse_duration(&text).map_err(serde::de::Error::custom),
        }
    }
}

fn parse_duration(text: &str) -> Result<DurationSeconds, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("duration cannot be empty".to_string());
    }

    let (digits, suffix) = trimmed.split_at(
        trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len()),
    );
    if digits.is_empty() {
        return Err(format!("duration must start with a number: {text}"));
    }

    let value = digits
        .parse::<u64>()
        .map_err(|error| format!("invalid duration number {digits}: {error}"))?;
    let multiplier = match suffix.trim() {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        "d" | "day" | "days" => 24 * 60 * 60,
        other => return Err(format!("unsupported duration suffix: {other}")),
    };

    value
        .checked_mul(multiplier)
        .map(DurationSeconds)
        .ok_or_else(|| format!("duration is too large: {text}"))
}

/// A local file-change gate for prompt context injection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileChangeMatcher {
    /// Glob pattern for changed files.
    #[serde(default)]
    pub file: GlobMatcher,

    /// How recently the file change must have been observed.
    #[serde(default)]
    pub within: DurationSeconds,
}

impl FileChangeMatcher {
    pub fn matches_change(&self, path: &str, age_seconds: u64) -> bool {
        self.file.is_match(path) && age_seconds <= self.within.as_secs()
    }
}

/// Example-based semantic intent matcher for `UserPromptSubmit`.
#[derive(Debug, Clone, Serialize)]
pub struct PromptIntentMatcher {
    examples: Vec<String>,
    threshold: f32,
}

impl PromptIntentMatcher {
    const DEFAULT_THRESHOLD: f32 = 0.60;

    pub fn matches_with_context(&self, prompt: &str) -> Vec<Match> {
        let prompt_tokens = normalized_tokens(prompt);
        let Some((example, score)) = self
            .examples
            .iter()
            .map(|example| {
                (
                    example,
                    semantic_score(&prompt_tokens, &normalized_tokens(example)),
                )
            })
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
        else {
            return Vec::new();
        };

        if score < self.threshold {
            return Vec::new();
        }

        let mut captures = Captures::new();
        captures.insert("intent_example".to_string(), example.clone());
        captures.insert("intent_score".to_string(), format!("{score:.2}"));

        vec![Match {
            span: Span::from(0..prompt.len()),
            captures,
        }]
    }
}

impl<'de> Deserialize<'de> for PromptIntentMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            examples: Vec<String>,
            threshold: Option<f32>,
        }

        let raw = Raw::deserialize(deserializer)?;
        if raw.examples.is_empty() {
            return Err(serde::de::Error::custom("intent examples cannot be empty"));
        }

        let threshold = raw.threshold.unwrap_or(Self::DEFAULT_THRESHOLD);
        if !(0.0..=1.0).contains(&threshold) {
            return Err(serde::de::Error::custom(
                "intent threshold must be between 0.0 and 1.0",
            ));
        }

        Ok(PromptIntentMatcher {
            examples: raw.examples,
            threshold,
        })
    }
}

fn semantic_score(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let intersection = left.intersection(right).count() as f32;
    let union = left.union(right).count() as f32;
    intersection / union
}

fn normalized_tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(canonical_token)
        .collect()
}

fn canonical_token(token: &str) -> Option<String> {
    let token = token.to_ascii_lowercase();
    match token.as_str() {
        "" | "a" | "an" | "and" | "are" | "can" | "could" | "do" | "does" | "for" | "how" | "i"
        | "if" | "is" | "let" | "lets" | "now" | "of" | "on" | "please" | "s" | "see"
        | "should" | "the" | "to" | "we" | "will" | "would" | "you" => None,

        "check" | "checking" | "execute" | "executing" | "pass" | "passes" | "passing" | "ran"
        | "run" | "running" | "runs" | "smoke" | "test" | "tested" | "testing" | "try"
        | "trying" | "validate" | "validating" | "verify" | "verifying" | "work" | "working"
        | "works" => Some("test".to_string()),

        "change" | "changes" | "current" | "implementation" | "it" | "local" | "locally"
        | "that" | "this" => Some("current".to_string()),

        other => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::*;

    #[test]
    fn parses_duration_strings() {
        pretty_assert_eq!(
            serde_yaml::from_str::<DurationSeconds>("30m")
                .expect("duration")
                .as_secs(),
            30 * 60
        );
        pretty_assert_eq!(
            serde_yaml::from_str::<DurationSeconds>("1h")
                .expect("duration")
                .as_secs(),
            60 * 60
        );
    }

    #[test]
    fn prompt_intent_matches_semantic_variants() {
        let matcher = serde_yaml::from_str::<PromptIntentMatcher>(
            r#"
examples:
  - "let's test this"
  - "try running it"
  - "does this work"
"#,
        )
        .expect("intent matcher");

        assert!(!matcher.matches_with_context("try executing it").is_empty());
        assert!(
            !matcher
                .matches_with_context("can we verify this now")
                .is_empty()
        );
        assert!(
            matcher
                .matches_with_context("run the unit tests")
                .is_empty()
        );
        assert!(
            matcher
                .matches_with_context("explain how this daemon is structured")
                .is_empty()
        );
    }
}
