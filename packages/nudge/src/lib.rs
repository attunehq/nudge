//! Main library for Nudge, used by its CLI.

pub mod claude;
pub mod rules;
pub mod snippet;

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
