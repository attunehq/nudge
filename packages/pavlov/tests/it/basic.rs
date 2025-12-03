//! Basic Rule Loading Tests

use crate::{assert_expected, run_hook, write_hook, Expected};
use xshell::Shell;

#[test]
fn test_no_rules_passthrough() {
    // Non-matching file extension should passthrough (no rules match)
    let sh = Shell::new().unwrap();
    let input = write_hook("test.xyz", "any content");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}
