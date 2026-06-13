//! Manage repo-local learned knowledge.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use color_eyre::eyre::{Context, Result};
use nudge::learn::{self, AddNote};

#[derive(Args, Clone, Debug)]
pub struct Config {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Clone, Debug)]
enum Commands {
    /// Add a learned incident note to .nudge/learned.
    Add(AddConfig),

    /// List learned incident notes.
    List(ListConfig),

    /// Search learned incident notes with BM25.
    Search(SearchConfig),
}

#[derive(Args, Clone, Debug)]
struct AddConfig {
    /// Short note title. If omitted, Nudge uses the first Markdown H1.
    #[arg(long)]
    title: Option<String>,

    /// Learned note body.
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,

    /// Read the learned note body from a file.
    #[arg(long, conflicts_with = "body")]
    body_file: Option<PathBuf>,
}

#[derive(Args, Clone, Debug)]
struct ListConfig {}

#[derive(Args, Clone, Debug)]
struct SearchConfig {
    /// Query text.
    #[arg(required = true)]
    query: Vec<String>,

    /// Maximum number of results to show.
    #[arg(long, default_value_t = learn::default_search_limit())]
    limit: usize,

    /// Minimum BM25 score to display.
    #[arg(long, default_value_t = 0.0)]
    min_score: f64,
}

pub fn main(config: Config) -> Result<()> {
    match config.command {
        Commands::Add(config) => add(config),
        Commands::List(config) => list(config),
        Commands::Search(config) => search(config),
    }
}

fn add(config: AddConfig) -> Result<()> {
    let body = learn::read_body(config.body, config.body_file)?;
    let path = learn::add(
        std::path::Path::new("."),
        AddNote {
            title: config.title,
            body,
        },
    )?;

    println!("Added learned note: {}", path.display());
    Ok(())
}

fn list(_config: ListConfig) -> Result<()> {
    let root = std::path::Path::new(".");
    let notes = learn::load_all(root).context("load learned notes")?;
    if notes.is_empty() {
        println!("No learned notes found in {}.", learn::LEARNED_DIR);
        return Ok(());
    }

    for (index, note) in notes.iter().enumerate() {
        println!(
            "{}. {}\n   Path: {}",
            index + 1,
            note.title,
            note.path.display()
        );
    }

    Ok(())
}

fn search(config: SearchConfig) -> Result<()> {
    let root = std::path::Path::new(".");
    let notes = learn::load_all(root).context("load learned notes")?;
    if notes.is_empty() {
        println!("No learned notes found in {}.", learn::LEARNED_DIR);
        return Ok(());
    }

    let query = config.query.join(" ");
    let results = learn::search(&notes, &query, config.limit, config.min_score);
    if results.is_empty() {
        println!("No learned notes matched the query.");
        return Ok(());
    }

    println!("{}", learn::render_search_results(root, &results));
    Ok(())
}
