//! Non-Rust File Tests

use crate::{Expected, assert_expected, run_hook, write_hook};
use simple_test_case::test_case;
use xshell::Shell;

#[test_case("test.py", "    use std::io;"; "python file passes")]
#[test_case("test.js", "    use std::io;"; "javascript file passes")]
#[test_case("test.txt", "let foo: Type = bar;"; "text file passes")]
#[test]
fn test_non_rust_files_pass(file_path: &str, content: &str) {
    let sh = Shell::new().unwrap();
    let input = write_hook(file_path, content);
    let (exit_code, output) = run_hook(&sh, &input);
    assert_expected(exit_code, &output, Expected::Passthrough);
}
