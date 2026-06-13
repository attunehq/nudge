//! Local embedding support for learned notes.

use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(feature = "embeddings")]
use std::collections::HashMap;

#[cfg(feature = "embeddings")]
use color_eyre::eyre::Context;
use color_eyre::eyre::{OptionExt, Result, bail};
#[cfg(feature = "embeddings")]
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(feature = "embeddings")]
use std::str::FromStr;

#[cfg(any(feature = "embeddings", test))]
use crate::learn::display_path;
#[cfg(feature = "embeddings")]
use crate::learn::search;
use crate::{
    learn::{LearnConfig, LearnedNote, SearchResult},
    rules,
};

#[cfg(feature = "embeddings")]
const INDEX_VERSION: u32 = 1;
const DEFAULT_MODEL: &str = "BGESmallENV15";
const DEFAULT_MODEL_NAME: &str = "BAAI/bge-small-en-v1.5";
#[cfg(feature = "embeddings")]
const SEMANTIC_WEIGHT: f64 = 2.5;
#[cfg(feature = "embeddings")]
const BM25_PREFETCH_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingPaths {
    pub cache_dir: PathBuf,
    pub model_dir: PathBuf,
    pub vector_index: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingStatus {
    pub available: bool,
    pub enabled: bool,
    pub model: String,
    pub paths: EmbeddingPaths,
    pub vector_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingBuildReport {
    pub model: String,
    pub paths: EmbeddingPaths,
    pub chunk_count: usize,
}

#[derive(Debug, Clone)]
#[cfg(any(feature = "embeddings", test))]
struct NoteChunk {
    note_path: PathBuf,
    title: String,
    text: String,
    note_hash: String,
    chunk_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VectorIndex {
    version: u32,
    project_root: String,
    model: String,
    chunks: Vec<VectorChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VectorChunk {
    note_path: String,
    title: String,
    text: String,
    note_hash: String,
    chunk_id: String,
    vector: Vec<f32>,
}

pub fn default_model() -> String {
    DEFAULT_MODEL_NAME.to_string()
}

pub fn is_available() -> bool {
    cfg!(feature = "embeddings")
}

pub fn unavailable_message() -> &'static str {
    "semantic embeddings are not available in this nudge binary; BM25 learned-note search remains available"
}

pub fn ensure_available() -> Result<()> {
    if is_available() {
        Ok(())
    } else {
        bail!("{}", unavailable_message())
    }
}

pub fn status(root: &Path, config: &LearnConfig) -> Result<EmbeddingStatus> {
    let paths = paths(root)?;
    let vector_count = load_index(&paths.vector_index)
        .map(|index| index.chunks.len())
        .unwrap_or(0);

    Ok(EmbeddingStatus {
        available: is_available(),
        enabled: config.embeddings.enabled,
        model: config.embeddings.model.clone(),
        paths,
        vector_count,
    })
}

pub fn paths(root: &Path) -> Result<EmbeddingPaths> {
    let dirs = rules::project_dirs().ok_or_eyre("resolve Nudge cache directory")?;
    let cache_dir = dirs.cache_dir().join("learn");
    let model_dir = cache_dir.join("models");
    let vector_dir = cache_dir.join("vectors");
    let vector_index = vector_dir.join(format!("{}.json", project_cache_key(root)));

    Ok(EmbeddingPaths {
        cache_dir,
        model_dir,
        vector_index,
    })
}

#[cfg(feature = "embeddings")]
pub fn build_index(
    root: &Path,
    notes: &[LearnedNote],
    config: &LearnConfig,
) -> Result<EmbeddingBuildReport> {
    let paths = paths(root)?;
    fs::create_dir_all(&paths.model_dir)
        .with_context(|| format!("create model cache dir: {}", paths.model_dir.display()))?;
    if let Some(parent) = paths.vector_index.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create vector cache dir: {}", parent.display()))?;
    }

    let chunks = chunks_for_notes(root, notes);
    let vectors = embed_texts(
        &config.embeddings.model,
        &paths.model_dir,
        chunks
            .iter()
            .map(|chunk| format!("passage: {}", chunk.text))
            .collect(),
    )?;
    let index = VectorIndex {
        version: INDEX_VERSION,
        project_root: canonical_root(root),
        model: canonical_model(&config.embeddings.model)?,
        chunks: chunks
            .into_iter()
            .zip(vectors)
            .map(|(chunk, vector)| VectorChunk {
                note_path: display_path(root, &chunk.note_path),
                title: chunk.title,
                text: chunk.text,
                note_hash: chunk.note_hash,
                chunk_id: chunk.chunk_id,
                vector,
            })
            .collect(),
    };
    let chunk_count = index.chunks.len();

    fs::write(
        &paths.vector_index,
        serde_json::to_string_pretty(&index).context("serialize embedding index")?,
    )
    .with_context(|| format!("write vector index: {}", paths.vector_index.display()))?;

    Ok(EmbeddingBuildReport {
        model: canonical_model(&config.embeddings.model)?,
        paths,
        chunk_count,
    })
}

#[cfg(not(feature = "embeddings"))]
pub fn build_index(
    _root: &Path,
    _notes: &[LearnedNote],
    _config: &LearnConfig,
) -> Result<EmbeddingBuildReport> {
    bail!("{}", unavailable_message())
}

#[cfg(feature = "embeddings")]
pub fn ensure_model(root: &Path, config: &LearnConfig) -> Result<EmbeddingPaths> {
    let paths = paths(root)?;
    fs::create_dir_all(&paths.model_dir)
        .with_context(|| format!("create model cache dir: {}", paths.model_dir.display()))?;
    let _ = embed_texts(
        &config.embeddings.model,
        &paths.model_dir,
        vec![String::from("query: nudge local embedding model warmup")],
    )?;
    Ok(paths)
}

#[cfg(not(feature = "embeddings"))]
pub fn ensure_model(_root: &Path, _config: &LearnConfig) -> Result<EmbeddingPaths> {
    bail!("{}", unavailable_message())
}

pub fn hybrid_search(
    root: &Path,
    notes: &[LearnedNote],
    query: &str,
    limit: usize,
    min_score: f64,
    config: &LearnConfig,
) -> Result<Option<Vec<SearchResult>>> {
    if !config.embeddings.enabled {
        return Ok(None);
    }

    if !is_available() {
        tracing::warn!(
            "learned-note embeddings are enabled in config but unavailable in this binary; falling back to BM25"
        );
        return Ok(None);
    }

    #[cfg(feature = "embeddings")]
    {
        hybrid_search_enabled(root, notes, query, limit, min_score, config)
    }
    #[cfg(not(feature = "embeddings"))]
    {
        let _ = (root, notes, query, limit, min_score, config);
        Ok(None)
    }
}

#[cfg(feature = "embeddings")]
fn hybrid_search_enabled(
    root: &Path,
    notes: &[LearnedNote],
    query: &str,
    limit: usize,
    min_score: f64,
    config: &LearnConfig,
) -> Result<Option<Vec<SearchResult>>> {
    let index = load_or_build_index(root, notes, config)?;
    if index.chunks.is_empty() {
        return Ok(None);
    }

    let paths = paths(root)?;
    let query_vector = embed_texts(
        &config.embeddings.model,
        &paths.model_dir,
        vec![format!("query: {query}")],
    )?
    .into_iter()
    .next()
    .ok_or_eyre("embedding model returned no query vector")?;

    let mut semantic_by_path = HashMap::<String, f64>::new();
    for chunk in &index.chunks {
        let similarity = cosine_similarity(&query_vector, &chunk.vector);
        semantic_by_path
            .entry(chunk.note_path.clone())
            .and_modify(|existing| *existing = existing.max(similarity))
            .or_insert(similarity);
    }

    let bm25 = search(notes, query, BM25_PREFETCH_LIMIT.max(limit), 0.0);
    let bm25_by_path = bm25
        .iter()
        .map(|result| (display_path(root, &result.path), result.score))
        .collect::<HashMap<_, _>>();

    let mut results = notes
        .iter()
        .filter_map(|note| {
            let note_path = display_path(root, &note.path);
            let bm25_score = bm25_by_path.get(&note_path).copied().unwrap_or(0.0);
            let semantic_score = semantic_by_path
                .get(&note_path)
                .copied()
                .unwrap_or(0.0)
                .max(0.0);
            let score = bm25_score + semantic_score * SEMANTIC_WEIGHT;
            (score >= min_score && score > 0.0).then(|| SearchResult {
                path: note.path.clone(),
                title: note.title.clone(),
                score,
                excerpt: crate::learn::excerpt(note, &crate::learn::query_terms(query)),
            })
        })
        .collect::<Vec<_>>();

    results.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    results.truncate(limit);

    Ok(Some(results))
}

#[cfg(feature = "embeddings")]
fn load_or_build_index(
    root: &Path,
    notes: &[LearnedNote],
    config: &LearnConfig,
) -> Result<VectorIndex> {
    let paths = paths(root)?;
    if let Some(index) =
        load_index(&paths.vector_index).filter(|index| index_is_current(root, notes, config, index))
    {
        return Ok(index);
    }

    build_index(root, notes, config)?;
    load_index(&paths.vector_index).ok_or_eyre("read rebuilt embedding index")
}

fn load_index(path: &Path) -> Option<VectorIndex> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

#[cfg(feature = "embeddings")]
fn index_is_current(
    root: &Path,
    notes: &[LearnedNote],
    config: &LearnConfig,
    index: &VectorIndex,
) -> bool {
    if index.version != INDEX_VERSION
        || index.project_root != canonical_root(root)
        || index.model != canonical_model(&config.embeddings.model).unwrap_or_default()
    {
        return false;
    }

    let expected = chunks_for_notes(root, notes)
        .into_iter()
        .map(|chunk| {
            (
                display_path(root, &chunk.note_path),
                chunk.chunk_id,
                chunk.note_hash,
            )
        })
        .collect::<Vec<_>>();
    let actual = index
        .chunks
        .iter()
        .map(|chunk| {
            (
                chunk.note_path.clone(),
                chunk.chunk_id.clone(),
                chunk.note_hash.clone(),
            )
        })
        .collect::<Vec<_>>();

    expected == actual
}

#[cfg(feature = "embeddings")]
fn embed_texts(model: &str, model_dir: &Path, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let model = canonical_model(model)?;
    let model = EmbeddingModel::from_str(&model)
        .map_err(|error| color_eyre::eyre::eyre!("unknown embedding model `{model}`: {error}"))?;
    let mut embedder = TextEmbedding::try_new(
        TextInitOptions::new(model)
            .with_cache_dir(model_dir.to_path_buf())
            .with_show_download_progress(false),
    )
    .map_err(|error| color_eyre::eyre::eyre!("initialize local embedding model: {error}"))?;

    embedder
        .embed(texts, None)
        .map_err(|error| color_eyre::eyre::eyre!("generate local embeddings: {error}"))
}

#[cfg(feature = "embeddings")]
pub fn canonical_model(model: &str) -> Result<String> {
    let canonical = canonical_model_alias(model);

    EmbeddingModel::from_str(&canonical)
        .map_err(|error| color_eyre::eyre::eyre!("unknown embedding model `{model}`: {error}"))?;

    Ok(canonical)
}

#[cfg(not(feature = "embeddings"))]
pub fn canonical_model(model: &str) -> Result<String> {
    Ok(canonical_model_alias(model))
}

fn canonical_model_alias(model: &str) -> String {
    let trimmed = model.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "baai/bge-small-en-v1.5" | "bge-small-en-v1.5" | "bgesmallenv15" => {
            String::from(DEFAULT_MODEL)
        }
        "sentence-transformers/all-minilm-l6-v2" | "all-minilm-l6-v2" | "allminilml6v2" => {
            String::from("AllMiniLML6V2")
        }
        "sentence-transformers/all-minilm-l6-v2-q" | "all-minilm-l6-v2-q" | "allminilml6v2q" => {
            String::from("AllMiniLML6V2Q")
        }
        _ => trimmed.to_string(),
    }
}

#[cfg(feature = "embeddings")]
fn chunks_for_notes(root: &Path, notes: &[LearnedNote]) -> Vec<NoteChunk> {
    notes
        .iter()
        .flat_map(|note| chunks_for_note(root, note))
        .collect()
}

#[cfg(any(feature = "embeddings", test))]
fn chunks_for_note(root: &Path, note: &LearnedNote) -> Vec<NoteChunk> {
    let body_without_title = note
        .body
        .lines()
        .skip_while(|line| line.trim().is_empty() || line.trim_start().starts_with("# "))
        .collect::<Vec<_>>()
        .join("\n");
    let note_hash = stable_hash(&note.body);
    let mut chunks = Vec::new();
    let mut heading = String::new();
    let mut section = Vec::new();

    for line in body_without_title.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") || trimmed.starts_with("### ") {
            push_section_chunk(root, note, &note_hash, &heading, &section, &mut chunks);
            heading = trimmed.trim_start_matches('#').trim().to_string();
            section.clear();
        } else {
            section.push(line.to_string());
        }
    }
    push_section_chunk(root, note, &note_hash, &heading, &section, &mut chunks);

    if chunks.is_empty() {
        chunks.push(NoteChunk {
            note_path: note.path.clone(),
            title: note.title.clone(),
            text: format!("{}\n{}", note.title, note.body.trim()),
            note_hash,
            chunk_id: stable_hash(&format!("{}:whole", display_path(root, &note.path))),
        });
    }

    chunks
}

#[cfg(any(feature = "embeddings", test))]
fn push_section_chunk(
    root: &Path,
    note: &LearnedNote,
    note_hash: &str,
    heading: &str,
    section: &[String],
    chunks: &mut Vec<NoteChunk>,
) {
    let section_text = section
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if heading.is_empty() && section_text.is_empty() {
        return;
    }

    let text = [note.title.as_str(), heading, section_text.as_str()]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let chunk_id = stable_hash(&format!(
        "{}:{}:{}",
        display_path(root, &note.path),
        heading,
        section_text
    ));

    chunks.push(NoteChunk {
        note_path: note.path.clone(),
        title: note.title.clone(),
        text,
        note_hash: note_hash.to_string(),
        chunk_id,
    });
}

#[cfg(feature = "embeddings")]
fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for (left, right) in left.iter().zip(right) {
        let left = *left as f64;
        let right = *right as f64;
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }

    dot / ((left_norm.sqrt() * right_norm.sqrt()) + f64::EPSILON)
}

fn canonical_root(root: &Path) -> String {
    root.canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string()
}

fn project_cache_key(root: &Path) -> String {
    stable_hash(&canonical_root(root))
}

fn stable_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn note(path: &str, title: &str, body: &str) -> LearnedNote {
        LearnedNote {
            path: PathBuf::from(path),
            title: title.to_string(),
            body: body.to_string(),
        }
    }

    #[test]
    fn chunks_notes_by_markdown_sections() {
        let root = TempDir::new().expect("temp dir");
        let note = note(
            root.path()
                .join(".nudge/learned/expo.md")
                .to_str()
                .expect("utf-8"),
            "Expo cache",
            "# Expo cache\n\n## What went wrong\n\nMetro reused stale resolver state.\n\n## Fix\n\nClear the cache.",
        );

        let chunks = chunks_for_note(root.path(), &note);

        pretty_assert_eq!(chunks.len(), 2);
        pretty_assert_eq!(chunks[0].note_path, note.path);
        pretty_assert_eq!(chunks[0].title, "Expo cache");
        assert!(chunks[0].text.contains("What went wrong"));
        assert!(!chunks[0].note_hash.is_empty());
        assert!(!chunks[0].chunk_id.is_empty());
        assert!(chunks[1].text.contains("Clear the cache"));
    }

    #[test]
    fn status_reports_configuration_without_requiring_model() {
        let root = TempDir::new().expect("temp dir");
        let config = LearnConfig::default();
        let status = status(root.path(), &config).expect("status");

        pretty_assert_eq!(status.available, cfg!(feature = "embeddings"));
        pretty_assert_eq!(status.enabled, false);
        pretty_assert_eq!(status.model, DEFAULT_MODEL_NAME);
    }

    #[test]
    fn canonical_model_accepts_hugging_face_name() {
        pretty_assert_eq!(
            canonical_model("BAAI/bge-small-en-v1.5").expect("canonical model"),
            "BGESmallENV15"
        );
    }
}
