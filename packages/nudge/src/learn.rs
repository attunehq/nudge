//! Repo-local learned knowledge storage and retrieval.

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Context, OptionExt, Result, bail};
use itertools::Itertools;
use walkdir::WalkDir;

use crate::hook::{NudgeHook, ToolUse};

pub const LEARNED_DIR: &str = ".nudge/learned";
const DEFAULT_SEARCH_LIMIT: usize = 5;
const HOOK_SEARCH_LIMIT: usize = 3;
const HOOK_MIN_SCORE: f64 = 1.2;
const HOOK_MIN_QUERY_TOKENS: usize = 3;
const EXCERPT_LIMIT: usize = 420;

/// A Markdown incident note stored under `.nudge/learned`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedNote {
    pub path: PathBuf,
    pub title: String,
    pub body: String,
}

/// A ranked learned-note match.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub path: PathBuf,
    pub title: String,
    pub score: f64,
    pub excerpt: String,
}

/// Learned context gathered for hook responses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookLearnedContext {
    pub user_prompt: Option<String>,
    pub pre_tool_use: Option<String>,
}

/// Options for adding a learned note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddNote {
    pub title: Option<String>,
    pub body: String,
}

pub fn learned_dir(root: &Path) -> PathBuf {
    root.join(LEARNED_DIR)
}

pub fn default_search_limit() -> usize {
    DEFAULT_SEARCH_LIMIT
}

/// Load all learned Markdown notes for a repo root.
pub fn load_all(root: &Path) -> Result<Vec<LearnedNote>> {
    let dir = learned_dir(root);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut notes = Vec::new();
    for entry in WalkDir::new(&dir).sort_by_file_name() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(?error, ?dir, "walking learned notes");
                continue;
            }
        };

        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|ext| ext != "md") {
            continue;
        }

        let body = fs::read_to_string(entry.path())
            .with_context(|| format!("read learned note: {}", entry.path().display()))?;
        let title = infer_title(&body).unwrap_or_else(|| {
            entry
                .path()
                .file_stem()
                .map(|stem| stem.to_string_lossy().replace('-', " "))
                .unwrap_or_else(|| String::from("learned note"))
        });

        notes.push(LearnedNote {
            path: entry.path().to_path_buf(),
            title,
            body,
        });
    }

    Ok(notes)
}

/// Add a Markdown learned note and return the created path.
pub fn add(root: &Path, note: AddNote) -> Result<PathBuf> {
    let body = normalize_newlines(note.body.trim());
    if body.trim().is_empty() {
        bail!("learned note body cannot be empty");
    }

    let title = note
        .title
        .filter(|title| !title.trim().is_empty())
        .map(|title| title.trim().to_string())
        .or_else(|| infer_title(&body))
        .ok_or_eyre("provide --title or start the note with a Markdown H1 heading")?;

    let content = if starts_with_h1(&body) {
        ensure_trailing_newline(&body)
    } else {
        format!("# {title}\n\n{}\n", body.trim())
    };

    let dir = learned_dir(root);
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;

    let path = next_available_path(&dir, &slugify(&title));
    fs::write(&path, content).with_context(|| format!("write learned note: {}", path.display()))?;

    Ok(path)
}

/// Read a note body from a CLI argument, file, or stdin.
pub fn read_body(body: Option<String>, body_file: Option<PathBuf>) -> Result<String> {
    match (body, body_file) {
        (Some(body), None) => Ok(body),
        (None, Some(path)) => {
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
        }
        (None, None) => {
            let stdin = io::stdin();
            if stdin.is_terminal() {
                bail!("provide --body, --body-file, or pipe note text on stdin");
            }

            let mut input = String::new();
            stdin
                .lock()
                .read_to_string(&mut input)
                .context("read note body from stdin")?;
            Ok(input)
        }
        (Some(_), Some(_)) => unreachable!("clap enforces body/body_file conflicts"),
    }
}

/// Search learned notes with a simple BM25 index built in memory.
pub fn search(
    notes: &[LearnedNote],
    query: &str,
    limit: usize,
    min_score: f64,
) -> Vec<SearchResult> {
    let query_tokens = tokenize(query);
    if notes.is_empty() || query_tokens.is_empty() || limit == 0 {
        return Vec::new();
    }

    let query_terms = query_tokens.into_iter().collect::<HashSet<_>>();
    let documents = notes
        .iter()
        .map(|note| IndexedDocument::new(note))
        .collect_vec();
    let average_length = documents
        .iter()
        .map(|document| document.length as f64)
        .sum::<f64>()
        / documents.len() as f64;
    let document_frequency = document_frequency(&documents);

    let mut results = notes
        .iter()
        .zip(documents.iter())
        .filter_map(|(note, document)| {
            let score = bm25_score(
                document,
                &query_terms,
                &document_frequency,
                notes.len(),
                average_length,
            );
            (score >= min_score && score > 0.0).then(|| SearchResult {
                path: note.path.clone(),
                title: note.title.clone(),
                score,
                excerpt: excerpt(note, &query_terms),
            })
        })
        .collect_vec();

    results.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    results.truncate(limit);
    results
}

/// Build learned context for hook responses.
pub fn context_for_hooks(
    root: &Path,
    hooks: &[NudgeHook],
    notes: &[LearnedNote],
) -> HookLearnedContext {
    if notes.is_empty() {
        return HookLearnedContext::default();
    }

    let user_prompt_query = hooks
        .iter()
        .filter_map(|hook| match hook {
            NudgeHook::UserPromptSubmit(payload) => Some(payload.prompt.as_str()),
            _ => None,
        })
        .join("\n\n");
    let pre_tool_query = hooks
        .iter()
        .filter_map(|hook| match hook {
            NudgeHook::PreToolUse(payload) => query_for_tool(&payload.tool),
            _ => None,
        })
        .join("\n\n");

    HookLearnedContext {
        user_prompt: hook_context_for_query(root, notes, &user_prompt_query),
        pre_tool_use: hook_context_for_query(root, notes, &pre_tool_query),
    }
}

pub fn render_search_results(root: &Path, results: &[SearchResult]) -> String {
    results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            format!(
                "{}. {}\n   Score: {:.2}\n   Path: {}\n   Excerpt: {}",
                index + 1,
                result.title,
                result.score,
                display_path(root, &result.path),
                result.excerpt
            )
        })
        .join("\n\n")
}

fn hook_context_for_query(root: &Path, notes: &[LearnedNote], query: &str) -> Option<String> {
    let query_tokens = tokenize(query);
    if query_tokens.len() < HOOK_MIN_QUERY_TOKENS {
        return None;
    }

    let results = search(notes, query, HOOK_SEARCH_LIMIT, HOOK_MIN_SCORE);
    if results.is_empty() {
        return None;
    }

    let rendered = render_search_results(root, &results);
    Some(format!(
        "Nudge found learned repo knowledge that may apply. Read this before repeating old debugging work:\n\n{rendered}"
    ))
}

fn query_for_tool(tool: &ToolUse) -> Option<String> {
    match tool {
        ToolUse::Bash(input) => Some(
            [Some(input.command.as_str()), input.description.as_deref()]
                .into_iter()
                .flatten()
                .join("\n"),
        ),
        ToolUse::WebFetch(input) => Some(
            [Some(input.url.as_str()), input.prompt.as_deref()]
                .into_iter()
                .flatten()
                .join("\n"),
        ),
        _ => None,
    }
}

fn document_frequency(documents: &[IndexedDocument]) -> HashMap<String, usize> {
    let mut frequency = HashMap::new();
    for document in documents {
        for term in document.term_frequency.keys() {
            *frequency.entry(term.clone()).or_insert(0) += 1;
        }
    }
    frequency
}

fn bm25_score(
    document: &IndexedDocument,
    query_terms: &HashSet<String>,
    document_frequency: &HashMap<String, usize>,
    document_count: usize,
    average_length: f64,
) -> f64 {
    const K1: f64 = 1.5;
    const B: f64 = 0.75;

    query_terms
        .iter()
        .filter_map(|term| {
            let term_frequency = *document.term_frequency.get(term)? as f64;
            let document_frequency = *document_frequency.get(term).unwrap_or(&0) as f64;
            let idf = ((document_count as f64 - document_frequency + 0.5)
                / (document_frequency + 0.5)
                + 1.0)
                .ln();
            let length_normalizer = K1
                * (1.0 - B + B * (document.length as f64 / average_length.max(f64::MIN_POSITIVE)));
            let score = idf * (term_frequency * (K1 + 1.0)) / (term_frequency + length_normalizer);
            Some(score)
        })
        .sum()
}

#[derive(Debug, Clone)]
struct IndexedDocument {
    term_frequency: HashMap<String, usize>,
    length: usize,
}

impl IndexedDocument {
    fn new(note: &LearnedNote) -> Self {
        let text = format!("{}\n{}\n{}", note.title, note.title, note.body);
        let tokens = tokenize(&text);
        let mut term_frequency = HashMap::new();
        for token in &tokens {
            *term_frequency.entry(token.clone()).or_insert(0) += 1;
        }

        Self {
            term_frequency,
            length: tokens.len(),
        }
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for character in text.chars() {
        if character.is_alphanumeric() {
            current.extend(character.to_lowercase());
        } else {
            push_token(&mut tokens, &mut current);
        }
    }
    push_token(&mut tokens, &mut current);

    tokens
}

fn push_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.len() >= 2 && !is_stop_word(current) {
        tokens.push(std::mem::take(current));
    } else {
        current.clear();
    }
}

fn is_stop_word(token: &str) -> bool {
    matches!(
        token,
        "about"
            | "after"
            | "again"
            | "also"
            | "and"
            | "are"
            | "because"
            | "been"
            | "before"
            | "but"
            | "can"
            | "could"
            | "did"
            | "does"
            | "doing"
            | "done"
            | "for"
            | "from"
            | "had"
            | "has"
            | "have"
            | "how"
            | "into"
            | "its"
            | "let"
            | "like"
            | "not"
            | "now"
            | "once"
            | "only"
            | "our"
            | "out"
            | "over"
            | "same"
            | "should"
            | "some"
            | "than"
            | "that"
            | "the"
            | "then"
            | "there"
            | "this"
            | "through"
            | "use"
            | "was"
            | "were"
            | "what"
            | "when"
            | "where"
            | "while"
            | "with"
            | "would"
            | "you"
            | "your"
    )
}

fn excerpt(note: &LearnedNote, query_terms: &HashSet<String>) -> String {
    let body_without_title = note
        .body
        .lines()
        .skip_while(|line| line.trim().is_empty() || line.trim_start().starts_with("# "))
        .join("\n");
    let paragraphs = body_without_title
        .split("\n\n")
        .map(clean_excerpt_text)
        .filter(|paragraph| !paragraph.is_empty())
        .collect_vec();

    let matching_index = paragraphs
        .iter()
        .position(|paragraph| {
            let paragraph_tokens = tokenize(paragraph).into_iter().collect::<HashSet<_>>();
            paragraph_tokens
                .iter()
                .any(|token| query_terms.contains(token))
        })
        .or_else(|| (!paragraphs.is_empty()).then_some(0));

    let Some(matching_index) = matching_index else {
        return String::new();
    };

    let mut selected = vec![paragraphs[matching_index].clone()];
    if let Some(fix) = paragraphs
        .iter()
        .enumerate()
        .find(|(index, paragraph)| {
            *index != matching_index && paragraph.to_lowercase().starts_with("fix")
        })
        .map(|(_, paragraph)| paragraph.clone())
    {
        selected.push(fix);
    }

    truncate_text(&selected.join(" "), EXCERPT_LIMIT)
}

fn clean_excerpt_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim())
        .join(" ")
}

fn truncate_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }

    let mut truncated = text
        .chars()
        .take(limit.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn infer_title(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.trim().strip_prefix("# ").map(str::trim))
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .or_else(|| {
            body.lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(|line| truncate_text(line.trim_start_matches('#').trim(), 80))
        })
}

fn starts_with_h1(body: &str) -> bool {
    body.lines()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.trim_start().starts_with("# "))
}

fn ensure_trailing_newline(body: &str) -> String {
    if body.ends_with('\n') {
        body.to_string()
    } else {
        format!("{body}\n")
    }
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_separator = false;

    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_was_separator = false;
        } else if !previous_was_separator && !slug.is_empty() {
            slug.push('-');
            previous_was_separator = true;
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        String::from("learned-note")
    } else {
        slug.to_string()
    }
}

fn next_available_path(dir: &Path, slug: &str) -> PathBuf {
    let first = dir.join(format!("{slug}.md"));
    if !first.exists() {
        return first;
    }

    for index in 2.. {
        let candidate = dir.join(format!("{slug}-{index}.md"));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("infinite iterator returns a path")
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq as pretty_assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn note(title: &str, body: &str) -> LearnedNote {
        LearnedNote {
            path: PathBuf::from(format!(".nudge/learned/{}.md", slugify(title))),
            title: title.to_string(),
            body: format!("# {title}\n\n{body}"),
        }
    }

    #[test]
    fn add_writes_markdown_note_with_slug() {
        let temp = TempDir::new().expect("temp dir");
        let path = add(
            temp.path(),
            AddNote {
                title: Some(String::from("Expo Metro Cache")),
                body: String::from("What went wrong\n\nThe resolver used stale state."),
            },
        )
        .expect("add note");

        pretty_assert_eq!(path, temp.path().join(".nudge/learned/expo-metro-cache.md"));
        let content = fs::read_to_string(path).expect("read note");
        assert!(content.starts_with("# Expo Metro Cache\n\n"));
        assert!(content.contains("The resolver used stale state."));
    }

    #[test]
    fn search_ranks_specific_incident_above_unrelated_note() {
        let notes = vec![
            note(
                "Expo Metro Cache",
                "Expo failed to resolve modules until Metro cache state was cleared.",
            ),
            note(
                "Rust Import Style",
                "Move use declarations to the top of Rust modules.",
            ),
        ];

        let results = search(
            &notes,
            "expo metro cannot resolve module after dependency update",
            5,
            0.0,
        );

        pretty_assert_eq!(results[0].title, "Expo Metro Cache");
        assert!(
            results
                .iter()
                .all(|result| result.title != "Rust Import Style"),
            "zero-score unrelated notes should not be returned"
        );
    }

    #[test]
    fn hook_context_skips_tiny_queries() {
        let notes = vec![note("Expo Metro Cache", "Clear Metro cache.")];
        let context = hook_context_for_query(Path::new("."), &notes, "expo");

        pretty_assert_eq!(context, None);
    }
}
