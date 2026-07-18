use std::{fmt, path::Path, sync::LazyLock};

use glob::Pattern;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeSeq};

/// Match paths against ordered include and exclusion glob patterns.
#[derive(Debug, Clone)]
pub struct GlobMatcher(Vec<PathPattern>);

#[derive(Debug, Clone)]
struct PathPattern {
    pattern: Pattern,
    effect: PatternEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternEffect {
    Include,
    Exclude,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawGlobMatcher {
    One(String),
    Many(Vec<String>),
}

impl GlobMatcher {
    /// Create an instance that matches any string.
    pub fn any() -> Self {
        static ANY: LazyLock<Pattern> =
            LazyLock::new(|| Pattern::new("**/*").expect("compile 'any' glob pattern"));
        GlobMatcher(vec![PathPattern {
            pattern: ANY.clone(),
            effect: PatternEffect::Include,
        }])
    }

    /// Test whether these patterns match a given string.
    pub fn is_match(&self, path: &str) -> bool {
        self.matches_with(|pattern| pattern.matches(path))
    }

    /// Test whether these patterns match the given path.
    pub fn is_match_path(&self, path: &Path) -> bool {
        self.matches_with(|pattern| pattern.matches_path(path))
    }

    fn matches_with(&self, matches: impl Fn(&Pattern) -> bool) -> bool {
        self.0
            .iter()
            .rev()
            .find(|entry| matches(&entry.pattern))
            .is_some_and(|entry| entry.effect == PatternEffect::Include)
    }

    fn parse_one(raw: String) -> Result<Self, glob::PatternError> {
        Pattern::new(&raw).map(|pattern| {
            Self(vec![PathPattern {
                pattern,
                effect: PatternEffect::Include,
            }])
        })
    }

    fn parse_many(raw_patterns: Vec<String>) -> Result<Self, String> {
        if raw_patterns.is_empty() {
            return Err(String::from("path glob list must not be empty"));
        }

        let patterns = raw_patterns
            .into_iter()
            .map(|raw| {
                let (effect, pattern) = match raw.strip_prefix('!') {
                    Some("") => {
                        return Err(String::from("path glob exclusion must include a pattern"));
                    }
                    Some(pattern) => (PatternEffect::Exclude, pattern),
                    None => (PatternEffect::Include, raw.as_str()),
                };

                Pattern::new(pattern)
                    .map(|pattern| PathPattern { pattern, effect })
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;

        if !patterns
            .iter()
            .any(|entry| entry.effect == PatternEffect::Include)
        {
            return Err(String::from(
                "path glob list must contain at least one positive pattern",
            ));
        }

        Ok(Self(patterns))
    }
}

impl fmt::Display for GlobMatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, entry) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            if entry.effect == PatternEffect::Exclude {
                formatter.write_str("!")?;
            }
            entry.pattern.fmt(formatter)?;
        }
        Ok(())
    }
}

impl Default for GlobMatcher {
    fn default() -> Self {
        Self::any()
    }
}

impl<'de> Deserialize<'de> for GlobMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RawGlobMatcher::deserialize(deserializer)? {
            RawGlobMatcher::One(raw) => Self::parse_one(raw).map_err(serde::de::Error::custom),
            RawGlobMatcher::Many(raw) => Self::parse_many(raw).map_err(serde::de::Error::custom),
        }
    }
}

impl Serialize for GlobMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let [entry] = self.0.as_slice()
            && entry.effect == PatternEffect::Include
        {
            return serializer.serialize_str(&entry.pattern.to_string());
        }

        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for entry in &self.0 {
            let pattern = entry.pattern.to_string();
            if entry.effect == PatternEffect::Exclude {
                sequence.serialize_element(&format_args!("!{pattern}"))?;
            } else {
                sequence.serialize_element(&pattern)?;
            }
        }
        sequence.end()
    }
}

#[cfg(test)]
mod tests;
