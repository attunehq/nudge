//! Display the syntax tree for a code snippet.

use std::path::Path;

use benchmark::{
    matcher::{FallibleMatcher, code::{CodeMatcher, Language, Query}},
    snippet::Snippet,
};
use clap::Args;
use color_eyre::Result;

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Language to parse the code as.
    #[arg(short, long)]
    language: Language,

    /// Tree-sitter query to run on the code. When provided, displays the source
    /// with matching spans highlighted instead of the syntax tree.
    #[arg(short, long)]
    query: Option<String>,

    /// Code to parse, or a path to a file containing the code.
    input: String,
}

pub fn main(config: Config) -> Result<()> {
    let code = resolve_input(&config.input);
    let snippet = Snippet::new(code);

    match config.query {
        Some(query_str) => {
            let query = Query::parse(config.language, query_str)?;
            let matcher = CodeMatcher::builder()
                .language(config.language)
                .query(query)
                .build();
            let matches = matcher.find(snippet.source())?;
            let spans = matches.spans().cloned().collect::<Vec<_>>();
            let highlighted = snippet.render_highlighted_green(spans);
            println!("{highlighted}");
        }
        None => {
            let tree = snippet.render_syntax_tree(config.language)?;
            println!("{tree}");
        }
    }
    Ok(())
}

/// Resolve the input as either a file path or literal code.
fn resolve_input(input: &str) -> String {
    let path = Path::new(input);
    if path.exists() {
        std::fs::read_to_string(path).unwrap_or_else(|_| input.to_string())
    } else {
        input.to_string()
    }
}
