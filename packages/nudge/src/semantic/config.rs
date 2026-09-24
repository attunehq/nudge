use serde::{Deserialize, Serialize};

use crate::rules::Language;

/// A natural-language lint predicate evaluated by Jev.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticConfig {
    pub select: Selector,
    pub violates: String,
    pub allow: String,
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Selector {
    Comments { language: Language },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    pub clear: f64,
    pub violation: f64,
}

impl SemanticConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(
            self.select,
            Selector::Comments {
                language: Language::Rust
            }
        ) {
            return Err(String::from(
                "Comments selection currently supports only Rust",
            ));
        }
        if self.violates.trim().is_empty() || self.allow.trim().is_empty() {
            return Err(String::from(
                "semantic violates and allow must be non-empty",
            ));
        }
        if !self.thresholds.clear.is_finite()
            || !self.thresholds.violation.is_finite()
            || self.thresholds.clear < 0.0
            || self.thresholds.violation > 1.0
            || self.thresholds.clear >= self.thresholds.violation
        {
            return Err(String::from(
                "semantic thresholds require 0 <= clear < violation <= 1",
            ));
        }
        Ok(())
    }
}
