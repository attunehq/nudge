//! Template interpolation for rule messages and suggestions.
//!
//! Supports:
//! - `{{ $1 }}`, `{{ $2 }}` - Positional capture groups
//! - `{{ $name }}` - Named capture groups
//! - `{{ suggestion }}` - Interpolate a suggestion template

use std::collections::HashMap;

/// Captures from a regex match for template interpolation.
///
/// Keys are strings: `"0"`, `"1"`, `"2"` for positional captures,
/// and the capture name for named captures (e.g., `"var_name"`).
pub type Captures = HashMap<String, String>;

/// Interpolate a template string with the given captures.
///
/// Supports:
/// - `{{ $0 }}`, `{{ $1 }}`, `{{ $2 }}`, etc. - Positional captures
/// - `{{ $name }}` - Named captures
/// - `{{ suggestion }}` - Special key for interpolated suggestions
///
/// Missing captures are left as-is in the template.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use nudge::template::interpolate;
///
/// let mut captures = HashMap::new();
/// captures.insert("1".to_string(), "foo".to_string());
/// captures.insert("var".to_string(), "bar".to_string());
///
/// let result = interpolate("Use {{ $var }} instead of {{ $1 }}", &captures);
/// assert_eq!(result, "Use bar instead of foo");
/// ```
pub fn interpolate(template: &str, captures: &Captures) -> String {
    let mut result = template.to_string();

    for (key, value) in captures {
        // For numbered captures, use {{ $N }} syntax
        // For named captures, use {{ $name }} syntax
        let pattern = format!("{{{{ ${} }}}}", key);
        result = result.replace(&pattern, value);
    }

    result
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::*;

    #[test]
    fn test_positional_interpolation() {
        let mut captures = Captures::new();
        captures.insert("0".to_string(), "foo.unwrap()".to_string());
        captures.insert("1".to_string(), "foo".to_string());

        let result = interpolate(
            "Replace {{ $1 }}.unwrap() with {{ $1 }}.expect()",
            &captures,
        );
        pretty_assert_eq!(result, "Replace foo.unwrap() with foo.expect()");
    }

    #[test]
    fn test_named_interpolation() {
        let mut captures = Captures::new();
        captures.insert("var".to_string(), "x".to_string());
        captures.insert("type".to_string(), "String".to_string());

        let result = interpolate("Variable {{ $var }} has type {{ $type }}", &captures);
        pretty_assert_eq!(result, "Variable x has type String");
    }

    #[test]
    fn test_missing_capture_left_asis() {
        let captures = Captures::new();
        let result = interpolate("Missing {{ $1 }} here", &captures);
        pretty_assert_eq!(result, "Missing {{ $1 }} here");
    }

    #[test]
    fn test_suggestion_interpolation() {
        let mut captures = Captures::new();
        captures.insert(
            "suggestion".to_string(),
            "use .expect() instead".to_string(),
        );

        let result = interpolate("Don't use .unwrap(). {{ $suggestion }}", &captures);
        pretty_assert_eq!(result, "Don't use .unwrap(). use .expect() instead");
    }

    #[test]
    fn test_mixed_captures() {
        let mut captures = Captures::new();
        captures.insert("0".to_string(), "foo.is_none()".to_string());
        captures.insert("1".to_string(), "foo".to_string());
        captures.insert("var".to_string(), "foo".to_string());

        let result = interpolate("let Some({{ $var }}) = {{ $var }} else { ... }", &captures);
        pretty_assert_eq!(result, "let Some(foo) = foo else { ... }");
    }

    #[test]
    fn test_empty_template() {
        let captures = Captures::new();
        let result = interpolate("", &captures);
        pretty_assert_eq!(result, "");
    }

    #[test]
    fn test_no_placeholders() {
        let mut captures = Captures::new();
        captures.insert("1".to_string(), "foo".to_string());

        let result = interpolate("No placeholders here", &captures);
        pretty_assert_eq!(result, "No placeholders here");
    }
}
