//! Rule evaluation for normalized hook events.

use color_eyre::eyre::Result;
use indoc::formatdoc;
use itertools::Itertools;
use serde_json::Value;

use crate::{
    hook::{
        BashInput, EditInput, NudgeHook, PreToolUse, ToolUse, UserPromptSubmit, WebFetchInput,
        WriteInput,
        response::HookOutcome,
        state::{FileChangeContext, InteractionState, now_seconds},
    },
    rules::{
        ContentMatcher, LoadedConfig, PreToolUseBashMatcher, PreToolUseEditMatcher,
        PreToolUseWebFetchMatcher, PreToolUseWriteMatcher, Rule, RuleAction, UrlMatcher,
        UserPromptSubmitMatcher,
    },
    snippet::{Annotation, Match, Source},
};

/// Evaluate a normalized hook batch against the full loaded config.
pub fn evaluate_config_hooks(hooks: &[NudgeHook], config: &LoadedConfig) -> Result<HookOutcome> {
    let mut state = InteractionState::default();
    evaluate_config_hooks_with_state(hooks, config, &mut state)
}

/// Evaluate a normalized hook batch against the full loaded config and local
/// interaction state.
pub fn evaluate_config_hooks_with_state(
    hooks: &[NudgeHook],
    config: &LoadedConfig,
    state: &mut InteractionState,
) -> Result<HookOutcome> {
    let rule_outcome = evaluate_hooks_with_state(hooks, &config.rules, state);
    let workflow_outcome = crate::workflow::evaluate_hooks(hooks, &config.workflows)?;

    Ok(combine_outcomes(rule_outcome, workflow_outcome))
}

fn combine_outcomes(rule_outcome: HookOutcome, workflow_outcome: HookOutcome) -> HookOutcome {
    match (rule_outcome, workflow_outcome) {
        (HookOutcome::Passthrough, workflow_outcome) => workflow_outcome,
        (rule_outcome, HookOutcome::Passthrough) => rule_outcome,
        (
            HookOutcome::AddContext {
                context: rule_context,
            },
            HookOutcome::AddContext {
                context: workflow_context,
            },
        ) => HookOutcome::AddContext {
            context: format!("{rule_context}\n\n{workflow_context}"),
        },
        (HookOutcome::AddContext { .. }, workflow_outcome @ HookOutcome::ContinueStop { .. }) => {
            workflow_outcome
        }
        (rule_outcome, _) => rule_outcome,
    }
}

/// Evaluate a normalized hook batch against loaded rules.
///
/// A raw provider hook can normalize into multiple Nudge events. This happens
/// for Codex `apply_patch`, where one tool call can touch several files.
pub fn evaluate_hooks(hooks: &[NudgeHook], rules: &[Rule]) -> HookOutcome {
    let mut state = InteractionState::default();
    evaluate_hooks_with_state(hooks, rules, &mut state)
}

/// Evaluate a normalized hook batch against loaded rules and local interaction
/// state.
pub fn evaluate_hooks_with_state(
    hooks: &[NudgeHook],
    rules: &[Rule],
    state: &mut InteractionState,
) -> HookOutcome {
    evaluate_hooks_at(hooks, rules, state, now_seconds())
}

fn evaluate_hooks_at(
    hooks: &[NudgeHook],
    rules: &[Rule],
    state: &mut InteractionState,
    now: u64,
) -> HookOutcome {
    let mut pretooluse_matches = Vec::new();
    let mut pretooluse_update = None;
    let mut user_prompt_matches = Vec::new();
    let mut user_prompt_updates = Vec::new();

    for hook in hooks {
        match hook {
            NudgeHook::PreToolUse(payload) => {
                let (payload, update) = apply_pretooluse_substitutions(payload, rules);
                pretooluse_update = pretooluse_update.or(update);
                pretooluse_matches.extend(evaluate_pretooluse(&payload, rules));
            }
            NudgeHook::UserPromptSubmit(payload) => {
                let (matches, updates) = evaluate_userpromptsubmit(payload, rules, state, now);
                user_prompt_matches.extend(matches);
                user_prompt_updates.extend(updates);
            }
            NudgeHook::PermissionRequest(_) | NudgeHook::Stop(_) | NudgeHook::Other => {}
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
        for update in user_prompt_updates {
            state.mark_reminder_shown(
                &update.cwd,
                &update.rule_name,
                now,
                update.change_id,
                update.change_timestamp,
            );
        }

        return HookOutcome::AddContext {
            context: user_prompt_matches.join("\n\n"),
        };
    }

    state.record_file_changes(hooks, rules, now);

    if let Some(update) = pretooluse_update {
        return HookOutcome::UpdatePreToolUse {
            system_message: update.user_message,
            additional_context: update.model_context,
            updated_input: update.updated_input,
        };
    }

    HookOutcome::Passthrough
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UserPromptStateUpdate {
    cwd: std::path::PathBuf,
    rule_name: String,
    change_id: Option<u64>,
    change_timestamp: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UserPromptRuleMatch {
    matches: Vec<Match>,
    state_update: Option<UserPromptStateUpdate>,
}

#[derive(Debug, Clone, PartialEq)]
struct PreToolUseSubstitution {
    updated_input: Value,
    user_message: String,
    model_context: String,
}

fn apply_pretooluse_substitutions(
    payload: &PreToolUse,
    rules: &[Rule],
) -> (PreToolUse, Option<PreToolUseSubstitution>) {
    let ToolUse::Bash(input) = &payload.tool else {
        return (payload.clone(), None);
    };

    let original_command = input.command.clone();
    let mut command = original_command.clone();

    for rule in rules
        .iter()
        .filter(|rule| rule.action == RuleAction::Substitute)
    {
        for matcher in rule.hooks_pretooluse_bash() {
            command = substitute_bash_command(payload, &command, matcher);
        }
    }

    if command == original_command {
        return (payload.clone(), None);
    }

    let mut updated_payload = payload.clone();
    if let ToolUse::Bash(input) = &mut updated_payload.tool {
        input.command = command.clone();
    }
    updated_payload.tool_input = updated_tool_input(&payload.tool_input, &command);

    let user_message = format!("Nudge substituted `{original_command}` -> `{command}`.");
    let model_context = format!(
        "Nudge rewrote the Bash command from `{original_command}` to `{command}` before execution."
    );

    (
        updated_payload.clone(),
        Some(PreToolUseSubstitution {
            updated_input: updated_payload.tool_input,
            user_message,
            model_context,
        }),
    )
}

fn substitute_bash_command(
    payload: &PreToolUse,
    command: &str,
    matcher: &PreToolUseBashMatcher,
) -> String {
    if !bash_project_state_matches(payload, matcher) {
        return command.to_string();
    }

    if evaluate_all_matched(command, &matcher.command).is_empty() {
        return command.to_string();
    }

    matcher
        .command
        .iter()
        .filter(|matcher| matcher.has_replacement())
        .fold(command.to_string(), |command, matcher| {
            matcher.replace_all(&command)
        })
}

fn updated_tool_input(tool_input: &Value, command: &str) -> Value {
    let mut updated_input = tool_input.clone();
    match &mut updated_input {
        Value::Object(map) => {
            map.insert("command".to_string(), Value::String(command.to_string()));
            updated_input
        }
        _ => serde_json::json!({ "command": command }),
    }
}

fn evaluate_pretooluse(payload: &PreToolUse, rules: &[Rule]) -> Vec<String> {
    let annotations = rules
        .iter()
        .filter(|rule| rule.action == RuleAction::Block)
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

fn evaluate_userpromptsubmit(
    payload: &UserPromptSubmit,
    rules: &[Rule],
    state: &InteractionState,
    now: u64,
) -> (Vec<String>, Vec<UserPromptStateUpdate>) {
    let mut updates = Vec::new();
    let mut annotations = Vec::new();
    for rule in rules {
        for matcher in rule.hooks_userpromptsubmit() {
            let Some(rule_match) = evaluate_user_prompt(payload, matcher, rule, state, now) else {
                continue;
            };
            if let Some(update) = rule_match.state_update {
                updates.push(update);
            }
            annotations.extend(rule.annotate_matches(rule_match.matches));
        }
    }

    if annotations.is_empty() {
        return (Vec::new(), Vec::new());
    }

    (
        vec![Source::from(&payload.prompt).annotate(annotations)],
        updates,
    )
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
        evaluate_all_file_matched(&input.file_path, &input.content, &matcher.content)
    } else {
        Vec::new()
    }
}

fn evaluate_edit(input: &EditInput, matcher: &PreToolUseEditMatcher) -> Vec<Match> {
    if matcher.file.is_match_path(&input.file_path) {
        evaluate_all_file_matched(&input.file_path, &input.new_string, &matcher.new_content)
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
    if !bash_project_state_matches(payload, matcher) {
        return Vec::new();
    }

    evaluate_all_matched(&input.command, &matcher.command)
}

fn bash_project_state_matches(payload: &PreToolUse, matcher: &PreToolUseBashMatcher) -> bool {
    matcher
        .project_state
        .iter()
        .all(|state_matcher| state_matcher.is_match(&payload.context.cwd))
}

fn evaluate_user_prompt(
    input: &UserPromptSubmit,
    matcher: &UserPromptSubmitMatcher,
    rule: &Rule,
    state: &InteractionState,
    now: u64,
) -> Option<UserPromptRuleMatch> {
    let mut matches = Vec::new();

    if !matcher.prompt.is_empty() {
        let prompt_matches = evaluate_all_matched(&input.prompt, &matcher.prompt);
        if prompt_matches.is_empty() {
            return None;
        }
        matches.extend(prompt_matches);
    }

    if let Some(intent) = &matcher.intent {
        let intent_matches = intent.matches_with_context(&input.prompt);
        if intent_matches.is_empty() {
            return None;
        }
        matches.extend(intent_matches);
    }

    if matcher.prompt.is_empty() && matcher.intent.is_none() {
        return None;
    }

    let change = if matcher.after_file_change.is_empty() {
        None
    } else {
        Some(state.latest_matching_file_change(
            &input.context.cwd,
            &matcher.after_file_change,
            now,
        )?)
    };

    if reminder_is_suppressed(input, matcher, rule, state, now, change.as_ref()) {
        return None;
    }

    if let Some(change) = &change {
        for prompt_match in &mut matches {
            prompt_match
                .captures
                .insert("changed_file".to_string(), change.path.clone());
            prompt_match
                .captures
                .insert("changed_at".to_string(), change.timestamp.to_string());
        }
    }

    let state_update = matcher
        .uses_interaction_state()
        .then(|| UserPromptStateUpdate {
            cwd: input.context.cwd.clone(),
            rule_name: rule.name.clone(),
            change_id: change.as_ref().map(|change| change.id),
            change_timestamp: change.map(|change| change.timestamp),
        });

    Some(UserPromptRuleMatch {
        matches,
        state_update,
    })
}

fn reminder_is_suppressed(
    input: &UserPromptSubmit,
    matcher: &UserPromptSubmitMatcher,
    rule: &Rule,
    state: &InteractionState,
    now: u64,
    change: Option<&FileChangeContext>,
) -> bool {
    let Some(reminder) = state.reminder(&input.context.cwd, &rule.name) else {
        return false;
    };

    if matcher.once_per_change {
        let Some(change) = change else {
            return true;
        };
        let already_seen = if let Some(last_change_id) = reminder.last_change_id {
            change.id <= last_change_id
        } else {
            reminder
                .last_change_timestamp
                .is_some_and(|last_change| change.timestamp <= last_change)
        };
        if already_seen {
            return true;
        }

        return false;
    }

    if let Some(cooldown) = matcher.cooldown
        && now.saturating_sub(reminder.last_shown_at) < cooldown.as_secs()
    {
        return true;
    }

    false
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
    evaluate_all_file_matched(std::path::Path::new(""), content, matchers)
}

/// Evaluate all content matchers with file path context and return matches only
/// if every matcher matched.
fn evaluate_all_file_matched(
    path: &std::path::Path,
    content: &str,
    matchers: &[ContentMatcher],
) -> Vec<Match> {
    let mut matches = Vec::new();
    for matcher in matchers {
        let matcher_matches = matcher.matches_with_path_context(Some(path), content);
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
    fn bash_substitution_updates_command_and_preserves_tool_input() {
        let hook = claude::parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "session_id": "test",
            "transcript_path": "/tmp/test",
            "permission_mode": "default",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": {
                "command": "npm install lodash",
                "description": "Install lodash",
                "timeout": 120
            }
        }))
        .expect("parse hook");

        let rules = rules(
            r#"
version: 1
rules:
  - name: yarn-add
    action: substitute
    description: Use yarn add instead of npm install
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install(?: (?P<args>.*))?$"
            replace: "yarn add {{ $args }}"
"#,
        );

        let HookOutcome::UpdatePreToolUse {
            updated_input,
            additional_context,
            ..
        } = evaluate_hooks(&hook, &rules)
        else {
            panic!("expected PreToolUse update");
        };

        pretty_assert_eq!(updated_input["command"], "yarn add lodash");
        pretty_assert_eq!(updated_input["description"], "Install lodash");
        pretty_assert_eq!(updated_input["timeout"], 120);
        assert!(additional_context.contains("npm install lodash"));
        assert!(additional_context.contains("yarn add lodash"));
    }

    #[test]
    fn substitution_happens_before_block_rules() {
        let hook = claude::parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": { "command": "npm run test" }
        }))
        .expect("parse hook");

        let rules = rules(
            r#"
version: 1
rules:
  - name: yarn-run
    action: substitute
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm run(?: (?P<args>.*))?$"
            replace: "yarn {{ $args }}"
  - name: block-npm
    message: "Use yarn."
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm\\b"
"#,
        );

        let HookOutcome::UpdatePreToolUse { updated_input, .. } = evaluate_hooks(&hook, &rules)
        else {
            panic!("expected PreToolUse update");
        };

        pretty_assert_eq!(updated_input["command"], "yarn test");
    }

    #[test]
    fn block_rules_after_substitution_override_update() {
        let hook = claude::parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": { "command": "npm install lodash" }
        }))
        .expect("parse hook");

        let rules = rules(
            r#"
version: 1
rules:
  - name: yarn-add
    action: substitute
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^npm install(?: (?P<args>.*))?$"
            replace: "yarn add {{ $args }}"
  - name: block-yarn-add
    message: "Do not install packages right now."
    on:
      - hook: PreToolUse
        tool: Bash
        command:
          - kind: Regex
            pattern: "^yarn add\\b"
"#,
        );

        assert!(matches!(
            evaluate_hooks(&hook, &rules),
            HookOutcome::DenyPreToolUse { .. }
        ));
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
