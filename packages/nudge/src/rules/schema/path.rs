use std::{path::Path, sync::LazyLock};

use derive_more::Display;
use glob::Pattern;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Match on a glob pattern.
#[derive(Debug, Clone, Display)]
#[display("{_0}")]
pub struct GlobMatcher(Pattern);

impl GlobMatcher {
    /// Create an instance that matches any string.
    pub fn any() -> Self {
        static ANY: LazyLock<Pattern> =
            LazyLock::new(|| Pattern::new("**/*").expect("compile 'any' glob pattern"));
        GlobMatcher(ANY.clone())
    }

    /// Test whether this pattern matches a given string.
    pub fn is_match(&self, path: &str) -> bool {
        self.0.matches(path)
    }

    /// Test whether this pattern matches the given path.
    pub fn is_match_path(&self, path: &Path) -> bool {
        self.0.matches_path(path)
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
        let s = String::deserialize(deserializer)?;
        Pattern::new(&s)
            .map(GlobMatcher)
            .map_err(serde::de::Error::custom)
    }
}

impl Serialize for GlobMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}
