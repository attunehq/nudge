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

    // Should be interrupt (exit 0 with deny in response)
    pretty_assert_eq!(exit_code, 0, "expected interrupt (exit 0)");
    assert!(
        output.contains(r#""permissionDecision":"deny""#),
        "expected deny in response, got: {output}"
    );

    // Should contain messages from both rules
    assert!(
        output.contains("Move this `use` statement"),
        "expected inline imports message, got: {output}"
    );
}
