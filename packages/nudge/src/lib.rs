//! Main library for Nudge, used by its CLI.

pub mod agent;
pub mod git;
pub mod hook;
pub mod learn;
pub mod rules;
pub mod snippet;
pub mod template;

/// Convenience macro for `filter_map`ing a pattern that contains a single item.
/// Returns `Some(item)` if the item matches the pattern, `None` otherwise.
#[macro_export]
macro_rules! fmap_match {
    ($($pattern:tt)+) => {
        |item| match item {
            $($pattern)+(item) => Some(item),
            _ => None,
        }
    }
}
