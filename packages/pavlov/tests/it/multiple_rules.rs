//! Multiple Rules Tests

use crate::{run_hook, write_hook};
use pretty_assertions::assert_eq as pretty_assert_eq;
use xshell::Shell;

#[test]
fn test_multiple_rules_fire() {
    let sh = Shell::new().unwrap();
    // Content that triggers both inline imports AND lhs type annotations
    let content = "fn main() {\n    use std::io;\n    let foo: Vec<String> = vec![];\n}";
    let input = write_hook("test.rs", content);
    let (exit_code, output) = run_hook(&sh, &input);

    // Should be interrupt (any interrupt = overall interrupt)
    pretty_assert_eq!(exit_code, 2, "expected interrupt when multiple rules fire");

    // Messages should be concatenated with separator
    assert!(
        output.contains("---") || output.contains("Move the `use` statement"),
        "expected messages from multiple rules"
    );
}
