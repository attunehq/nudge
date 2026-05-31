use std::{collections::HashSet, sync::LazyLock};

use crate::{
    snippet::{Match, Span},
    template::Captures,
};

use super::Language;

const MAX_COMMENT_LINES: usize = 2;
const MAX_COMMENT_TOKENS: usize = 14;
const MAX_CODE_LINES: usize = 5;

#[derive(Clone, Copy, Debug)]
struct SourceLine<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

#[derive(Clone, Copy, Debug)]
struct LineComment<'a> {
    marker_start: usize,
    end: usize,
    text: &'a str,
}

#[derive(Debug)]
struct CommentGroup {
    span: Span,
    text: String,
    line_count: usize,
    has_metadata_label: bool,
}

pub(super) fn matches(language: Language, source: &str) -> Vec<Match> {
    let lines = source_lines(source);
    let mut matches = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let Some((group, next_index)) = comment_group(language, &lines, index) else {
            index += 1;
            continue;
        };

        index = next_index;

        if group.line_count > MAX_COMMENT_LINES
            || group.has_metadata_label
            || next_index >= lines.len()
            || lines[next_index].text.trim().is_empty()
            || line_comment(language, lines[next_index]).is_some()
        {
            continue;
        }

        let code = code_window(language, &lines, next_index);
        if let Some(matched_tokens) = obvious_restatement(&group.text, &code) {
            let mut captures = Captures::new();
            captures.insert("comment".to_string(), group.text.clone());
            captures.insert("code".to_string(), code.trim().to_string());
            captures.insert("matched_tokens".to_string(), matched_tokens.join(", "));
            matches.push(Match {
                span: group.span,
                captures,
            });
        }
    }

    matches
}

fn source_lines(source: &str) -> Vec<SourceLine<'_>> {
    let mut lines = Vec::new();
    let mut start = 0;

    for segment in source.split_inclusive('\n') {
        let without_lf = segment.strip_suffix('\n').unwrap_or(segment);
        let text = without_lf.strip_suffix('\r').unwrap_or(without_lf);
        let end = start + text.len();
        lines.push(SourceLine { start, end, text });
        start += segment.len();
    }

    if !source.is_empty() && !source.ends_with('\n') && lines.is_empty() {
        lines.push(SourceLine {
            start: 0,
            end: source.len(),
            text: source,
        });
    }

    lines
}

fn comment_group(
    language: Language,
    lines: &[SourceLine<'_>],
    start_index: usize,
) -> Option<(CommentGroup, usize)> {
    let first = line_comment(language, lines[start_index])?;
    let mut parts = Vec::from([first.text.trim().to_string()]);
    let mut end = first.end;
    let mut line_count = 1;
    let mut contains_metadata_label = has_metadata_label(first.text);
    let mut index = start_index + 1;

    while index < lines.len() {
        let Some(comment) = line_comment(language, lines[index]) else {
            break;
        };

        parts.push(comment.text.trim().to_string());
        end = comment.end;
        line_count += 1;
        contains_metadata_label |= has_metadata_label(comment.text);
        index += 1;
    }

    Some((
        CommentGroup {
            span: Span {
                start: first.marker_start,
                end,
            },
            text: parts.join(" ").trim().to_string(),
            line_count,
            has_metadata_label: contains_metadata_label,
        },
        index,
    ))
}

fn line_comment<'a>(language: Language, line: SourceLine<'a>) -> Option<LineComment<'a>> {
    let trimmed = line.text.trim_start();
    let leading = line.text.len() - trimmed.len();
    let prefix = line_comment_prefix(language);

    if !trimmed.starts_with(prefix) || is_doc_comment(language, trimmed) {
        return None;
    }

    let text = trimmed[prefix.len()..].trim_start();
    Some(LineComment {
        marker_start: line.start + leading,
        end: line.end,
        text,
    })
}

fn line_comment_prefix(language: Language) -> &'static str {
    match language {
        Language::Python => "#",
        Language::Haskell => "--",
        Language::Rust
        | Language::TypeScript
        | Language::JavaScript
        | Language::Go
        | Language::Java
        | Language::CSharp
        | Language::Kotlin => "//",
    }
}

fn is_doc_comment(language: Language, trimmed: &str) -> bool {
    match language {
        Language::Rust => {
            trimmed.starts_with("///")
                || trimmed.starts_with("//!")
                || trimmed.starts_with("/**")
                || trimmed.starts_with("/*!")
        }
        Language::Java | Language::JavaScript | Language::TypeScript | Language::Go => {
            trimmed.starts_with("/**")
        }
        Language::Kotlin | Language::CSharp => {
            trimmed.starts_with("///") || trimmed.starts_with("/**")
        }
        Language::Python => {
            trimmed.starts_with("#!")
                || trimmed.starts_with("# type:")
                || trimmed.starts_with("# noqa")
                || trimmed.starts_with("# pylint:")
                || trimmed.starts_with("# fmt:")
        }
        Language::Haskell => trimmed.starts_with("-- |") || trimmed.starts_with("-- ^"),
    }
}

fn code_window(language: Language, lines: &[SourceLine<'_>], start_index: usize) -> String {
    let first = lines[start_index].text.trim();
    let mut code = first.to_string();

    if !opens_block(language, first) {
        return code;
    }

    for line in lines.iter().skip(start_index + 1).take(MAX_CODE_LINES - 1) {
        let trimmed = line.text.trim();
        if trimmed.is_empty() || line_comment(language, *line).is_some() {
            break;
        }

        code.push('\n');
        code.push_str(trimmed);

        if closes_block(language, trimmed) {
            break;
        }
    }

    code
}

fn opens_block(language: Language, code: &str) -> bool {
    let trimmed = code.trim();
    match language {
        Language::Python => trimmed.ends_with(':'),
        Language::Haskell => false,
        _ => {
            let opens = trimmed.matches('{').count();
            let closes = trimmed.matches('}').count();
            opens > closes
        }
    }
}

fn closes_block(language: Language, code: &str) -> bool {
    match language {
        Language::Python => false,
        Language::Haskell => false,
        _ => code.trim().contains('}'),
    }
}

fn obvious_restatement(comment: &str, code: &str) -> Option<Vec<String>> {
    if has_why_signal(comment) {
        return None;
    }

    let comment_tokens = meaningful_tokens(comment);
    let comment_set = token_set(&comment_tokens);
    if comment_set.len() < 2 || comment_set.len() > MAX_COMMENT_TOKENS {
        return None;
    }

    let code_set = code_token_set(code);
    if code_set.is_empty() || !is_actionish(&comment_tokens) {
        return None;
    }

    let mut matched = comment_set
        .intersection(&code_set)
        .cloned()
        .collect::<Vec<_>>();
    matched.sort();

    let overlap = matched.len();
    let ratio = overlap as f32 / comment_set.len() as f32;
    let first = comment_tokens
        .first()
        .map(String::as_str)
        .unwrap_or_default();
    let first_action_matches = strong_action_verbs().contains(first) && code_set.contains(first);

    if (overlap >= 3 && ratio >= 0.50)
        || (overlap >= 2 && ratio >= 0.67)
        || (comment_set.len() <= 3 && overlap >= 1 && first_action_matches)
    {
        Some(matched)
    } else {
        None
    }
}

fn has_metadata_label(comment: &str) -> bool {
    let normalized = comment.trim_start().to_ascii_lowercase();
    let labels = [
        "todo",
        "fixme",
        "safety",
        "note",
        "hack",
        "warning",
        "warn",
        "invariant",
        "perf",
        "security",
        "compat",
        "legacy",
    ];

    labels.iter().any(|label| {
        normalized == *label
            || normalized
                .strip_prefix(label)
                .is_some_and(|rest| rest.starts_with(':') || rest.starts_with('('))
    })
}

fn has_why_signal(comment: &str) -> bool {
    let lower = comment.to_ascii_lowercase();
    let phrases = [
        "because",
        "so that",
        "in order to",
        "to avoid",
        "to prevent",
        "to ensure",
        "must not",
        "must never",
        "do not",
        "don't",
        "cannot",
        "can't",
        "never ",
        "without ",
    ];

    if phrases.iter().any(|phrase| lower.contains(phrase)) {
        return true;
    }

    let tokens = token_set(&meaningful_tokens(comment));
    why_tokens().iter().any(|token| tokens.contains(*token))
}

fn meaningful_tokens(text: &str) -> Vec<String> {
    raw_words(text)
        .into_iter()
        .flat_map(|word| token_variants(&word))
        .filter(|token| !stop_words().contains(token.as_str()))
        .collect()
}

fn raw_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else {
            push_split_word(&mut words, &current);
            current.clear();
        }
    }

    push_split_word(&mut words, &current);
    words
}

fn push_split_word(words: &mut Vec<String>, raw: &str) {
    if raw.is_empty() {
        return;
    }

    let mut start = 0;
    let chars = raw.char_indices().collect::<Vec<_>>();
    for window in chars.windows(2) {
        let (_, ch) = window[0];
        let (next_index, next) = window[1];
        let split = (ch.is_lowercase() && next.is_uppercase())
            || (ch.is_alphabetic() && next.is_ascii_digit())
            || (ch.is_ascii_digit() && next.is_alphabetic());

        if split {
            words.push(raw[start..next_index].to_ascii_lowercase());
            start = next_index;
        }
    }

    words.push(raw[start..].to_ascii_lowercase());
}

fn token_variants(word: &str) -> Vec<String> {
    let normalized = normalize_token(word);
    let mut variants = Vec::from([normalized.clone()]);

    if let Some(stripped) = normalized.strip_suffix("ies") {
        variants.push(format!("{stripped}y"));
    } else if let Some(stripped) = normalized.strip_suffix("ing")
        && stripped.len() > 2
    {
        variants.push(stripped.to_string());
        variants.push(format!("{stripped}e"));
    } else if let Some(stripped) = normalized.strip_suffix("ed")
        && stripped.len() > 2
    {
        variants.push(stripped.to_string());
    } else if let Some(stripped) = normalized.strip_suffix("es")
        && stripped.len() > 2
    {
        variants.push(stripped.to_string());
    } else if let Some(stripped) = normalized.strip_suffix('s')
        && stripped.len() > 2
    {
        variants.push(stripped.to_string());
    }

    variants.sort();
    variants.dedup();
    variants
}

fn normalize_token(word: &str) -> String {
    match word {
        "func" => "function".to_string(),
        "fn" => "function".to_string(),
        "const" | "let" | "var" => "set".to_string(),
        other => other.to_string(),
    }
}

fn token_set(tokens: &[String]) -> HashSet<String> {
    tokens.iter().cloned().collect()
}

fn code_token_set(code: &str) -> HashSet<String> {
    let mut tokens = meaningful_tokens(code);
    let lower = code.to_ascii_lowercase();

    if looks_like_assignment(&lower) {
        tokens.extend(["set".to_string(), "assign".to_string()]);
    }

    if looks_like_loop(&lower) {
        tokens.extend([
            "loop".to_string(),
            "iterate".to_string(),
            "iteration".to_string(),
        ]);
    }

    if looks_like_condition(&lower) {
        tokens.extend([
            "if".to_string(),
            "condition".to_string(),
            "conditional".to_string(),
        ]);
    }

    if looks_like_call(&lower) {
        tokens.extend(["call".to_string(), "function".to_string()]);
    }

    token_set(&tokens)
}

fn looks_like_assignment(code: &str) -> bool {
    code.contains(" = ")
        || code.contains(":=")
        || code.contains("+=")
        || code.contains("-=")
        || code.contains("*=")
        || code.contains("/=")
        || code.trim_start().starts_with("let ")
        || code.trim_start().starts_with("const ")
        || code.trim_start().starts_with("var ")
}

fn looks_like_loop(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.starts_with("for ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("loop ")
        || trimmed.starts_with("for(")
        || trimmed.starts_with("while(")
}

fn looks_like_condition(code: &str) -> bool {
    let trimmed = code.trim_start();
    trimmed.starts_with("if ")
        || trimmed.starts_with("if(")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("switch ")
}

fn looks_like_call(code: &str) -> bool {
    code.contains('(') && !looks_like_loop(code) && !looks_like_condition(code)
}

fn is_actionish(tokens: &[String]) -> bool {
    let Some(first) = tokens.first().map(String::as_str) else {
        return false;
    };

    action_verbs().contains(first)
        || tokens
            .iter()
            .any(|token| control_words().contains(token.as_str()))
}

fn stop_words() -> &'static HashSet<&'static str> {
    static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
        HashSet::from([
            "a", "an", "and", "are", "as", "at", "be", "by", "each", "for", "from", "in", "into",
            "is", "it", "its", "of", "on", "one", "or", "the", "then", "these", "this", "those",
            "through", "to", "true", "with",
        ])
    });
    &STOP_WORDS
}

fn action_verbs() -> &'static HashSet<&'static str> {
    static ACTION_VERBS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
        HashSet::from([
            "add",
            "append",
            "assign",
            "build",
            "call",
            "calculate",
            "check",
            "close",
            "compute",
            "convert",
            "create",
            "delete",
            "deserialize",
            "dispatch",
            "fetch",
            "filter",
            "format",
            "get",
            "handle",
            "hash",
            "if",
            "init",
            "initialize",
            "insert",
            "iterate",
            "load",
            "log",
            "loop",
            "open",
            "parse",
            "print",
            "process",
            "push",
            "read",
            "remove",
            "rename",
            "render",
            "return",
            "save",
            "send",
            "serialize",
            "set",
            "sort",
            "update",
            "use",
            "validate",
            "write",
        ])
    });
    &ACTION_VERBS
}

fn strong_action_verbs() -> &'static HashSet<&'static str> {
    static STRONG_ACTION_VERBS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
        HashSet::from([
            "assign", "call", "check", "delete", "get", "if", "iterate", "loop", "process",
            "remove", "rename", "return", "set", "update",
        ])
    });
    &STRONG_ACTION_VERBS
}

fn control_words() -> &'static HashSet<&'static str> {
    static CONTROL_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
        HashSet::from([
            "call",
            "condition",
            "conditional",
            "each",
            "function",
            "if",
            "iterate",
            "loop",
            "match",
            "switch",
            "while",
        ])
    });
    &CONTROL_WORDS
}

fn why_tokens() -> &'static HashSet<&'static str> {
    static WHY_TOKENS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
        HashSet::from([
            "atomic",
            "avoid",
            "backwards",
            "borrow",
            "cache",
            "compatible",
            "compatibility",
            "concurrent",
            "concurrency",
            "constraint",
            "database",
            "deadlock",
            "deterministic",
            "expensive",
            "fast",
            "guarantee",
            "idempotent",
            "invariant",
            "latency",
            "legacy",
            "limit",
            "lock",
            "memory",
            "migration",
            "migrations",
            "mutex",
            "overflow",
            "panic",
            "partial",
            "performance",
            "plaintext",
            "pool",
            "prevent",
            "race",
            "regression",
            "safety",
            "security",
            "serial",
            "slow",
            "stable",
            "thread",
            "unsafe",
            "workaround",
        ])
    });
    &WHY_TOKENS
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;

    use super::*;

    #[test]
    fn matches_obvious_call_comment() {
        let source = "// Rename the temp file to the target path\n\
                      tokio::fs::rename(temp_path, target).await?;\n";

        let matches = matches(Language::Rust, source);

        pretty_assert_eq!(matches.len(), 1);
        pretty_assert_eq!(
            matches[0].captures.get("comment"),
            Some(&"Rename the temp file to the target path".to_string())
        );
    }

    #[test]
    fn preserves_why_comment() {
        let source = "// Use atomic rename to prevent partial reads during concurrent access\n\
                      tokio::fs::rename(temp_path, target).await?;\n";

        assert!(matches(Language::Rust, source).is_empty());
    }

    #[test]
    fn requires_adjacent_code() {
        let source = "// Rename the temp file to the target path\n\
                      \n\
                      tokio::fs::rename(temp_path, target).await?;\n";

        assert!(matches(Language::Rust, source).is_empty());
    }

    #[test]
    fn supports_hash_comments() {
        let source = "# Set x to 5\nx = 5\n";

        let matches = matches(Language::Python, source);

        pretty_assert_eq!(matches.len(), 1);
    }
}
