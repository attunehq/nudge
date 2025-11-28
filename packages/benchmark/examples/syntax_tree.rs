use color_eyre::Result;
use indoc::indoc;

use benchmark::matcher::code::Language;
use benchmark::snippet::Snippet;

const SOURCE: &str = indoc! {r#"
fn main() -> Result<()> {
    println!("Hello world");
    Ok(())
}
"#};

fn main() -> Result<()> {
    let snippet = Snippet::new(SOURCE);

    let source = snippet.render();
    let tree = snippet.render_syntax_tree(Language::Rust)?;

    println!("Source code:");
    println!("{source}");
    println!();
    println!("============");
    println!("Syntax tree:");
    println!("{tree}");
    println!();

    Ok(())
}
