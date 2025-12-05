//! Edit Tool Tests

use crate::{Expected, assert_expected, edit_hook, run_hook};
use xshell::Shell;

#[test]
fn test_edit_tool_content_matching() {
    let sh = Shell::new().unwrap();
    // Edit that introduces an indented use statement
    let input = edit_hook("test.rs", "old code", "    use std::io;\n");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn test_edit_tool_non_matching() {
    let sh = Shell::new().unwrap();
    // Edit that doesn't trigger any rules
    let input = edit_hook("test.rs", "old", "new");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}
