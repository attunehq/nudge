//! Inline Imports Rule Tests

use crate::{Expected, assert_expected, run_hook, write_hook};
use simple_test_case::test_case;
use xshell::Shell;

#[test_case(
    "fn main() {\n    use std::io;\n}",
    Expected::Interrupt;
    "indented use statement triggers interrupt"
)]
#[test_case(
    "use std::io;\n\nfn main() {}",
    Expected::Passthrough;
    "top-level use statement passes"
)]
#[test]
fn test_inline_imports(content: &str, expected: Expected) {
    let sh = Shell::new().unwrap();
    let input = write_hook("test.rs", content);
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, expected);
}
