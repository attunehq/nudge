use std::path::Path;

use pretty_assertions::assert_eq as pretty_assert_eq;

use super::GlobMatcher;

#[test]
fn scalar_glob_preserves_literal_leading_bang() {
    let matcher = serde_yaml::from_str::<GlobMatcher>("'!important.ts'").expect("valid glob");

    assert!(matcher.is_match("!important.ts"));
    assert!(!matcher.is_match("important.ts"));
}

#[test]
fn ordered_patterns_exclude_and_reinclude_paths() {
    let matcher = serde_yaml::from_str::<GlobMatcher>(
        r#"
            - "**/*.ts"
            - "!**/*.gen.ts"
            - "src/kept.gen.ts"
        "#,
    )
    .expect("valid ordered globs");

    assert!(matcher.is_match_path(Path::new("src/main.ts")));
    assert!(matcher.is_match_path(Path::new("/repo/src/main.ts")));
    assert!(!matcher.is_match_path(Path::new("src/routeTree.gen.ts")));
    assert!(!matcher.is_match_path(Path::new("/repo/src/routeTree.gen.ts")));
    assert!(matcher.is_match_path(Path::new("src/kept.gen.ts")));
    assert!(!matcher.is_match_path(Path::new("src/main.js")));
}

#[test]
fn later_positive_pattern_overrides_earlier_exclusion() {
    let matcher = serde_yaml::from_str::<GlobMatcher>(
        r#"
            - "!vendor/**"
            - "**/*.ts"
        "#,
    )
    .expect("valid ordered globs");

    assert!(matcher.is_match_path(Path::new("vendor/main.ts")));
}

#[test]
fn empty_and_all_negative_lists_are_rejected() {
    let empty = serde_yaml::from_str::<GlobMatcher>("[]").expect_err("empty list must fail");
    let all_negative = serde_yaml::from_str::<GlobMatcher>(
        r#"
            - "!**/*.gen.ts"
        "#,
    )
    .expect_err("all-negative list must fail");
    let bare_exclusion =
        serde_yaml::from_str::<GlobMatcher>("- '!' ").expect_err("bare exclusion must fail");

    assert!(empty.to_string().contains("must not be empty"));
    assert!(
        all_negative
            .to_string()
            .contains("must contain at least one positive pattern")
    );
    assert!(
        bare_exclusion
            .to_string()
            .contains("exclusion must include a pattern")
    );
}

#[test]
fn invalid_patterns_are_rejected() {
    let scalar = serde_yaml::from_str::<GlobMatcher>("'['").expect_err("invalid scalar must fail");
    let list = serde_yaml::from_str::<GlobMatcher>(
        r#"
            - "**/*.ts"
            - "!["
        "#,
    )
    .expect_err("invalid exclusion must fail");

    assert!(scalar.to_string().contains("Pattern syntax error"));
    assert!(list.to_string().contains("Pattern syntax error"));
}

#[test]
fn serialization_keeps_scalars_and_ordered_lists() {
    let scalar = serde_yaml::from_str::<GlobMatcher>("'**/*.rs'").expect("valid scalar glob");
    let list = serde_yaml::from_str::<GlobMatcher>(
        r#"
            - "**/*.ts"
            - "!**/*.gen.ts"
        "#,
    )
    .expect("valid glob list");

    pretty_assert_eq!(
        serde_yaml::to_string(&scalar).expect("serialize scalar"),
        "'**/*.rs'\n"
    );
    pretty_assert_eq!(
        serde_yaml::to_string(&list).expect("serialize list"),
        "- '**/*.ts'\n- '!**/*.gen.ts'\n"
    );
}
