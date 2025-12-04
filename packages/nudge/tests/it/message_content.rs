//! Message Content Tests

use crate::{run_hook, write_hook};
use xshell::Shell;

#[test]
fn test_interrupt_message_contains_rule_message() {
    let sh = Shell::new().unwrap();
    let input = write_hook("test.rs", "fn main() {\n    use std::io;\n}");
    let (_, output) = run_hook(&sh, &input);

    // Should contain the message from the no-inline-imports rule
    assert!(
        output.contains("Move this `use` statement"),
        "expected rule message in output, got: {output}"
    );
}
