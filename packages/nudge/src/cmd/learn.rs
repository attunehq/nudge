//! Manage repo-local learned knowledge.

use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Args, Subcommand};
use color_eyre::eyre::{Context, OptionExt, Result, bail};
use nudge::{
    learn::{self, AddNote, LearnConfig},
    skills,
};
use serde_yaml::{Mapping, Value};

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

    /// Search learned incident notes.
    Search(SearchConfig),

    /// Print the bundled Nudge learned-note guidance.
    Docs(DocsConfig),

    /// Manage local semantic embeddings for learned notes.
    Embeddings(EmbeddingsConfig),
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

    /// Minimum score to display.
    #[arg(long, default_value_t = 0.0)]
    min_score: f64,
}

#[derive(Args, Clone, Debug)]
struct DocsConfig {}

#[derive(Args, Clone, Debug)]
struct EmbeddingsConfig {
    #[command(subcommand)]
    command: EmbeddingsCommands,
}

#[derive(Subcommand, Clone, Debug)]
enum EmbeddingsCommands {
    /// Enable local embeddings in the project Nudge config and build the index.
    Enable(EnableEmbeddingsConfig),

    /// Rebuild the user-level vector cache for this project.
    Reindex(ReindexEmbeddingsConfig),

    /// Show embedding configuration and cache paths.
    Status(StatusEmbeddingsConfig),
}

#[derive(Args, Clone, Debug)]
struct EnableEmbeddingsConfig {
    /// Local embedding model to use.
    #[arg(long, default_value_t = learn::embeddings::default_model())]
    model: String,

    /// Update config without downloading the model or building vectors.
    #[arg(long)]
    no_reindex: bool,
}

#[derive(Args, Clone, Debug)]
struct ReindexEmbeddingsConfig {}

#[derive(Args, Clone, Debug)]
struct StatusEmbeddingsConfig {}

pub fn main(config: Config) -> Result<()> {
    match config.command {
        Commands::Add(config) => add(config),
        Commands::List(config) => list(config),
        Commands::Search(config) => search(config),
        Commands::Docs(config) => docs(config),
        Commands::Embeddings(config) => embeddings(config),
    }
}

fn add(config: AddConfig) -> Result<()> {
    let body = learn::read_body(config.body, config.body_file)?;
    let path = learn::add(
        Path::new("."),
        AddNote {
            title: config.title,
            body,
        },
    )?;

    println!("Added learned note: {}", path.display());
    Ok(())
}

fn list(_config: ListConfig) -> Result<()> {
    let root = Path::new(".");
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
    let root = Path::new(".");
    let notes = learn::load_all(root).context("load learned notes")?;
    if notes.is_empty() {
        println!("No learned notes found in {}.", learn::LEARNED_DIR);
        return Ok(());
    }

    let query = config.query.join(" ");
    let learn_config = learn::load_config().context("load learn config")?;
    let results = learn::search_with_config(
        root,
        &notes,
        &query,
        config.limit,
        config.min_score,
        &learn_config,
    )?;
    if results.is_empty() {
        println!("No learned notes matched the query.");
        return Ok(());
    }

    println!("{}", learn::render_search_results(root, &results));
    Ok(())
}

fn docs(_config: DocsConfig) -> Result<()> {
    println!("{}", skills::render_nudge_learnings_docs());
    Ok(())
}

fn embeddings(config: EmbeddingsConfig) -> Result<()> {
    match config.command {
        EmbeddingsCommands::Enable(config) => embeddings_enable(config),
        EmbeddingsCommands::Reindex(config) => embeddings_reindex(config),
        EmbeddingsCommands::Status(config) => embeddings_status(config),
    }
}

fn embeddings_enable(config: EnableEmbeddingsConfig) -> Result<()> {
    learn::embeddings::ensure_available()?;
    let model = learn::embeddings::canonical_model(&config.model)?;
    let learn_config = LearnConfig {
        embeddings: learn::EmbeddingConfig {
            enabled: true,
            model,
        },
    };
    let config_path = write_project_learn_config(&learn_config)?;
    println!(
        "Enabled local learned-note embeddings in {}.",
        config_path.display()
    );

    if !config.no_reindex {
        rebuild_embeddings(&learn_config)?;
    }

    Ok(())
}

fn embeddings_reindex(_config: ReindexEmbeddingsConfig) -> Result<()> {
    learn::embeddings::ensure_available()?;
    let config = learn::load_config().context("load learn config")?;
    if !config.embeddings.enabled {
        bail!("learn embeddings are not enabled. Run `nudge learn embeddings enable` first.");
    }

    rebuild_embeddings(&config)
}

fn embeddings_status(_config: StatusEmbeddingsConfig) -> Result<()> {
    let root = Path::new(".");
    let config = learn::load_config().context("load learn config")?;
    let status = learn::embeddings::status(root, &config)?;

    println!(
        "Embedding support: {}",
        if status.available {
            "available"
        } else {
            "unavailable in this binary"
        }
    );
    println!(
        "Embeddings: {}",
        if status.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!("Model: {}", status.model);
    println!("Cache: {}", status.paths.cache_dir.display());
    println!("Model cache: {}", status.paths.model_dir.display());
    println!("Vector index: {}", status.paths.vector_index.display());
    println!("Vectors: {}", status.vector_count);
    if status.enabled && !status.available {
        println!(
            "Semantic search is unavailable in this binary. BM25 learned-note search remains available."
        );
    }

    Ok(())
}

fn rebuild_embeddings(config: &LearnConfig) -> Result<()> {
    let root = Path::new(".");
    let notes = learn::load_all(root).context("load learned notes")?;
    if notes.is_empty() {
        let paths = learn::embeddings::ensure_model(root, config)?;
        println!(
            "Downloaded local embedding model. No learned notes found yet, so no vector index was built."
        );
        println!("Model cache: {}", paths.model_dir.display());
        return Ok(());
    }

    let report = learn::embeddings::build_index(root, &notes, config)?;
    println!(
        "Built learned-note vector index with {} chunks using {}.",
        report.chunk_count, report.model
    );
    println!("Model cache: {}", report.paths.model_dir.display());
    println!("Vector index: {}", report.paths.vector_index.display());

    Ok(())
}

fn write_project_learn_config(config: &LearnConfig) -> Result<PathBuf> {
    let path = project_config_path();
    let mut value = if path.exists() {
        serde_yaml::from_str::<Value>(
            &fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?,
        )
        .with_context(|| format!("parse {}", path.display()))?
    } else {
        Value::Mapping(Mapping::new())
    };

    let mapping = value
        .as_mapping_mut()
        .ok_or_eyre("project Nudge config must be a YAML mapping")?;
    mapping
        .entry(Value::String(String::from("version")))
        .or_insert(serde_yaml::to_value(1)?);
    mapping
        .entry(Value::String(String::from("rules")))
        .or_insert(Value::Sequence(Vec::new()));
    mapping.insert(
        Value::String(String::from("learn")),
        serde_yaml::to_value(config).context("serialize learn config")?,
    );

    fs::write(&path, serde_yaml::to_string(&value)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

fn project_config_path() -> PathBuf {
    let yaml = PathBuf::from(".nudge.yaml");
    if yaml.exists() {
        return yaml;
    }

    let yml = PathBuf::from(".nudge.yml");
    if yml.exists() {
        return yml;
    }

    yaml
}
