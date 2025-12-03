//! UserPromptSubmit Hook Tests

use crate::{assert_expected, run_hook, user_prompt_hook, Expected};
use xshell::Shell;

#[test]
fn test_user_prompt_no_matching_rules() {
    let sh = Shell::new().unwrap();
    let input = user_prompt_hook("hello world");
    let (exit_code, output) = run_hook(&sh, &input);
    // No UserPromptSubmit rules in the test config, so should passthrough
    assert_expected(exit_code, &output, Expected::Passthrough);
}
