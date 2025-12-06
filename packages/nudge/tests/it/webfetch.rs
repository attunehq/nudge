//! WebFetch Tool Tests

use crate::{assert_expected, run_hook, webfetch_hook, Expected};
use xshell::Shell;

#[test]
fn test_webfetch_docs_rs_triggers_interrupt() {
    let sh = Shell::new().unwrap();
    // WebFetch to docs.rs should trigger the rule
    let input = webfetch_hook("https://docs.rs/serde/1.0.0/serde/", "What does this crate do?");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Interrupt);
}

#[test]
fn test_webfetch_other_url_passes() {
    let sh = Shell::new().unwrap();
    // WebFetch to a non-matched URL should pass through
    let input = webfetch_hook("https://example.com/page", "What is this?");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}

#[test]
fn test_webfetch_captures_crate_name() {
    let sh = Shell::new().unwrap();
    // WebFetch to docs.rs should capture the crate name in the message
    let input = webfetch_hook("https://docs.rs/tokio/1.0.0/tokio/", "Tell me about tokio");
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Interrupt);
    // The message should contain the interpolated crate name
    assert!(
        output.contains("tokio"),
        "Expected output to contain crate name 'tokio', got: {output}"
    );
}
