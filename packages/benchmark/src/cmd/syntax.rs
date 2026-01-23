//! Display the syntax tree for a code snippet.

use std::fs;
use std::path::Path;

use benchmark::{matcher::code::Language, snippet::Snippet};
use clap::Args;
use color_eyre::Result;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Language to parse the code as.
    #[arg(short, long)]
    language: Language,

    /// Code to parse, or a path to a file containing the code.
    input: String,
}

pub fn main(config: Config) -> Result<()> {
    let code = resolve_input(&config.input);
    let snippet = Snippet::new(code);
    let tree = snippet.render_syntax_tree(config.language)?;
    println!("{tree}");
    Ok(())
}

/// Resolve the input as either a file path or literal code.
fn resolve_input(input: &str) -> String {
    let path = Path::new(input);
    if path.exists() {
        fs::read_to_string(path).unwrap_or_else(|_| input.to_string())
    } else {
        input.to_string()
    }
}
