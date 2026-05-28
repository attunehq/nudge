//! Rule evaluation for normalized hook events.

use indoc::formatdoc;
use itertools::Itertools;

use crate::{
    hook::{
        BashInput, EditInput, NudgeHook, PreToolUse, ToolUse, UserPromptSubmit, WebFetchInput,
        WriteInput, response::HookOutcome,
    },
    rules::{
        ContentMatcher, PreToolUseBashMatcher, PreToolUseEditMatcher, PreToolUseWebFetchMatcher,
        PreToolUseWriteMatcher, Rule, UrlMatcher, UserPromptSubmitMatcher,
    },
    snippet::{Annotation, Match, Source},
};

/// Evaluate a normalized hook batch against loaded rules.
///
/// A raw provider hook can normalize into multiple Nudge events. This happens
/// for Codex `apply_patch`, where one tool call can touch several files.
pub fn evaluate_hooks(hooks: &[NudgeHook], rules: &[Rule]) -> HookOutcome {
    let mut pretooluse_matches = Vec::new();
    let mut user_prompt_matches = Vec::new();

    for hook in hooks {
        match hook {
            NudgeHook::PreToolUse(payload) => {
                pretooluse_matches.extend(evaluate_pretooluse(payload, rules));
            }
            NudgeHook::UserPromptSubmit(payload) => {
                user_prompt_matches.extend(evaluate_userpromptsubmit(payload, rules));
            }
            NudgeHook::PermissionRequest(_) | NudgeHook::Other => {}
        }
    }

    if !pretooluse_matches.is_empty() {
        let matches = pretooluse_matches.join("\n\n");
        return HookOutcome::DenyPreToolUse {
            message: formatdoc! {"
                Nudge blocked operation due to rule violation.
                Fix all issues immediately and try again:

                {matches}
            "},
        };
    }

    if !user_prompt_matches.is_empty() {
        return HookOutcome::AddContext {
            context: user_prompt_matches.join("\n\n"),
        };
    }

    HookOutcome::Passthrough
}

fn evaluate_pretooluse(payload: &PreToolUse, rules: &[Rule]) -> Vec<String> {
    let annotations = rules
        .iter()
        .flat_map(|rule| match &payload.tool {
            ToolUse::Write(input) => annotate_write(rule, input),
            ToolUse::Edit(input) => annotate_edit(rule, input),
            ToolUse::WebFetch(input) => annotate_webfetch(rule, input),
            ToolUse::Bash(input) => annotate_bash(rule, payload, input),
            ToolUse::Delete(_) | ToolUse::Other { .. } => Vec::new(),
        })
        .collect_vec();

    if annotations.is_empty() {
        return Vec::new();
    }

    vec![source_for_tool(&payload.tool).annotate(annotations)]
}

fn evaluate_userpromptsubmit(payload: &UserPromptSubmit, rules: &[Rule]) -> Vec<String> {
    let annotations = rules
        .iter()
        .flat_map(|rule| {
            rule.hooks_userpromptsubmit()
                .flat_map(|matcher| evaluate_user_prompt(payload, matcher))
                .pipe_matches(rule)
        })
        .collect_vec();

    if annotations.is_empty() {
        return Vec::new();
    }

    vec![Source::from(&payload.prompt).annotate(annotations)]
}

trait PipeMatches: Iterator<Item = Match> + Sized {
    fn pipe_matches(self, rule: &Rule) -> Vec<Annotation> {
        rule.annotate_matches(self).collect_vec()
    }
}

impl<T> PipeMatches for T where T: Iterator<Item = Match> {}

fn annotate_write(rule: &Rule, input: &WriteInput) -> Vec<Annotation> {
    rule.hooks_pretooluse_write()
        .flat_map(|matcher| evaluate_write(input, matcher))
        .pipe_matches(rule)
}

fn annotate_edit(rule: &Rule, input: &EditInput) -> Vec<Annotation> {
    rule.hooks_pretooluse_edit()
        .flat_map(|matcher| evaluate_edit(input, matcher))
        .pipe_matches(rule)
}

fn annotate_webfetch(rule: &Rule, input: &WebFetchInput) -> Vec<Annotation> {
    rule.hooks_pretooluse_webfetch()
        .flat_map(|matcher| evaluate_webfetch(input, matcher))
        .pipe_matches(rule)
}

fn annotate_bash(rule: &Rule, payload: &PreToolUse, input: &BashInput) -> Vec<Annotation> {
    rule.hooks_pretooluse_bash()
        .flat_map(|matcher| evaluate_bash(payload, input, matcher))
        .pipe_matches(rule)
}

fn evaluate_write(input: &WriteInput, matcher: &PreToolUseWriteMatcher) -> Vec<Match> {
    if matcher.file.is_match_path(&input.file_path) {
        evaluate_all_matched(&input.content, &matcher.content)
    } else {
        Vec::new()
    }
}

fn evaluate_edit(input: &EditInput, matcher: &PreToolUseEditMatcher) -> Vec<Match> {
    if matcher.file.is_match_path(&input.file_path) {
        evaluate_all_matched(&input.new_string, &matcher.new_content)
    } else {
        Vec::new()
    }
}

fn evaluate_webfetch(input: &WebFetchInput, matcher: &PreToolUseWebFetchMatcher) -> Vec<Match> {
    evaluate_all_url_matched(&input.url, &matcher.url)
}

fn evaluate_bash(
    payload: &PreToolUse,
    input: &BashInput,
    matcher: &PreToolUseBashMatcher,
) -> Vec<Match> {
    for state_matcher in &matcher.project_state {
        if !state_matcher.is_match(&payload.context.cwd) {
            return Vec::new();
        }
    }

    evaluate_all_matched(&input.command, &matcher.command)
}

fn evaluate_user_prompt(input: &UserPromptSubmit, matcher: &UserPromptSubmitMatcher) -> Vec<Match> {
    evaluate_all_matched(&input.prompt, &matcher.prompt)
}

fn source_for_tool(tool: &ToolUse) -> Source {
    match tool {
        ToolUse::Write(input) => Source::from(&input.content),
        ToolUse::Edit(input) => Source::from(&input.new_string),
        ToolUse::Delete(_) | ToolUse::Other { .. } => Source::from(""),
        ToolUse::WebFetch(input) => Source::from(&input.url),
        ToolUse::Bash(input) => Source::from(&input.command),
    }
}

/// Evaluate all content matchers and return matches only if every matcher
/// matched.
fn evaluate_all_matched(content: &str, matchers: &[ContentMatcher]) -> Vec<Match> {
    let mut matches = Vec::new();
    for matcher in matchers {
        let matcher_matches = matcher.matches_with_context(content);
        if matcher_matches.is_empty() {
            return Vec::new();
        }
        matches.extend(matcher_matches);
    }
    matches
}

/// Evaluate all URL matchers and return matches only if every matcher matched.
fn evaluate_all_url_matched(url: &str, matchers: &[UrlMatcher]) -> Vec<Match> {
    let mut matches = Vec::new();
    for matcher in matchers {
        let matcher_matches = matcher.matches_with_context(url);
        if matcher_matches.is_empty() {
            return Vec::new();
        }
        matches.extend(matcher_matches);
    }
    matches
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use pretty_assertions::assert_eq as pretty_assert_eq;

    use crate::{
        agent::{claude, codex},
        hook::{NudgeHook, ToolUse, evaluate::evaluate_hooks, response::HookOutcome},
        rules::RuleConfig,
    };

    fn rules(yaml: &str) -> Vec<crate::rules::Rule> {
        serde_yaml::from_str::<RuleConfig>(yaml)
            .expect("rules parse")
            .rules
    }

    #[test]
    fn permission_request_passes_through() {
        let hook = claude::parse_hook(json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "test",
            "transcript_path": "/tmp/test",
            "permission_mode": "default",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": { "command": "rm -rf target" }
        }))
        .expect("parse hook");

        assert!(matches!(hook.as_slice(), [NudgeHook::PermissionRequest(_)]));

        let rules = rules(
            r#"
version: 1
rules:
  - name: block-rm
    message: "No rm"
    description: test
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "rm"
"#,
        );

        pretty_assert_eq!(evaluate_hooks(&hook, &rules), HookOutcome::Passthrough);
    }

    #[test]
    fn delete_is_normalized_but_unmatchable() {
        let hook = codex::parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "apply_patch",
            "tool_input": {
                "command": "*** Begin Patch\n*** Delete File: test.rs\n*** End Patch\n"
            }
        }))
        .expect("parse hook");

        assert!(matches!(
            hook.as_slice(),
            [NudgeHook::PreToolUse(crate::hook::PreToolUse {
                tool: ToolUse::Delete(_),
                ..
            })]
        ));

        let rules = rules(
            r#"
version: 1
rules:
  - name: no-inline-imports
    message: "Move import"
    description: test
    on:
      - hook: PreToolUse
        tool: Write
        file: "**/*.rs"
        content:
          - kind: Regex
            pattern: ".*"
      - hook: PreToolUse
        tool: Edit
        file: "**/*.rs"
        new_content:
          - kind: Regex
            pattern: ".*"
"#,
        );

        pretty_assert_eq!(evaluate_hooks(&hook, &rules), HookOutcome::Passthrough);
    }
}
