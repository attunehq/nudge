use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use pretty_assertions::assert_eq as pretty_assert_eq;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::{
    agent::{claude, codex, cursor, grok},
    hook::{NudgeHook, ToolUse, evaluate::evaluate_hooks_with_transport, response::HookOutcome},
    rules::{Rule, RuleConfig},
};

use super::{client, edit, select, *};

const RULE: &str = r#"
version: 1
rules:
  - name: explain-intent
    action: warn
    message: Remove redundant comments or explain the constraint.
    on:
      - hook: PreToolUse
        tool: Write
        file: '**/*.rs'
        semantic:
          select: {kind: Comments, language: rust}
          violates: The comment merely restates the code.
          allow: The comment explains intent, a constraint, or useful context.
          thresholds: {clear: 0.1, violation: 0.9}
"#;

fn rules(yaml: &str) -> Vec<Rule> {
    serde_yaml::from_str::<RuleConfig>(yaml)
        .expect("valid rules")
        .rules
}

#[derive(Default)]
struct Fake {
    probability: f64,
    requests: Vec<Value>,
    error: Option<EvaluationError>,
}

impl Transport for Fake {
    fn send(&mut self, request: &Value, _: Instant) -> Result<Value, EvaluationError> {
        self.requests.push(request.clone());
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let answers = request["questions"]
            .as_object()
            .expect("questions")
            .keys()
            .map(|id| (id.clone(), json!({"type":"noul", "noul":self.probability})))
            .collect::<serde_json::Map<_, _>>();
        Ok(json!({"model":client::MODEL,"answers":answers}))
    }
}

fn write_hook(code: &str) -> Vec<NudgeHook> {
    claude::parse_hook(
        json!({"hook_event_name":"PreToolUse","tool_name":"Write","cwd":"/tmp",
        "tool_input":{"file_path":"src.rs","content":code}}),
    )
    .expect("write hook")
}

#[test]
fn validates_semantic_rules_without_credentials() {
    pretty_assert_eq!(rules(RULE).len(), 1);
    for invalid in [
        RULE.replace("action: warn", "action: block"),
        RULE.replace("action: warn", "action: substitute"),
        RULE.replace("language: rust", "language: python"),
        RULE.replace("clear: 0.1", "clear: 0.95"),
        RULE.replace("clear: 0.1", "clear: .nan"),
        RULE.replace("violation: 0.9", "violation: 1.1"),
        RULE.replace(
            "violates: The comment merely restates the code.",
            "violates: ''",
        ),
        RULE.replace(
            "semantic:",
            "semantic:\n          endpoint: https://example.com",
        ),
    ] {
        assert!(
            serde_yaml::from_str::<RuleConfig>(&invalid).is_err(),
            "accepted {invalid}"
        );
    }
}

#[test]
fn extracts_grouped_comments_with_unicode_but_not_docs_or_strings() {
    let code = "/// API contract\nfn grace() {\n    let text = \"// not a comment\";\n    // café\n    // explanation\n    run();\n}\n";
    let candidates = select::comments(code).expect("candidates");
    pretty_assert_eq!(candidates.len(), 1);
    assert!(code[candidates[0].span.range()].contains("café"));
    pretty_assert_eq!(candidates[0].state["code"], "run();");
    pretty_assert_eq!(
        select::comments("fn ada() { let text = \"// hi\"; }")
            .expect("parse")
            .len(),
        0
    );
    assert!(select::comments("fn ada( {").is_err());
    assert!(select::comments("// standalone").is_err());
}

#[test]
fn warning_policy_and_batching_preserve_distinct_predicates() {
    let hooks = write_hook("fn grace() {\n// Call run\nrun();\n// Call stop\nstop();\n}\n");
    for (probability, status) in [
        (0.1, None),
        (0.5, Some(Status::Uncertain)),
        (0.9, Some(Status::Finding)),
    ] {
        let mut fake = Fake {
            probability,
            ..Fake::default()
        };
        let diagnostics =
            Plan::from_hooks(&hooks, &rules(RULE)).execute(&mut fake, Instant::now() + HOOK_BUDGET);
        pretty_assert_eq!(fake.requests.len(), 1);
        pretty_assert_eq!(
            fake.requests[0]["questions"]
                .as_object()
                .expect("questions")
                .len(),
            2
        );
        match status {
            None => assert!(diagnostics.is_empty()),
            Some(status) => {
                pretty_assert_eq!(diagnostics.len(), 2);
                assert!(diagnostics.iter().all(|d| d.status == status));
            }
        }
    }
    let mut fake = Fake {
        probability: 0.99,
        ..Fake::default()
    };
    assert!(matches!(
        evaluate_hooks_with_transport(&hooks, &rules(RULE), &mut fake),
        HookOutcome::AllowPreToolUseWithContext { .. }
    ));
}

#[test]
fn missing_context_preconditions_and_no_candidates_do_not_call_jev() {
    let mut fake = Fake::default();
    let precondition = RULE.replace(
        "        semantic:",
        "        content:\n          - kind: Regex\n            pattern: NEVER\n        semantic:",
    );
    let hooks = write_hook("fn ada() {\n// Call run\nrun();\n}");
    pretty_assert_eq!(
        evaluate_hooks_with_transport(&hooks, &rules(&precondition), &mut fake),
        HookOutcome::Passthrough
    );
    pretty_assert_eq!(
        evaluate_hooks_with_transport(&write_hook("fn ada() {}"), &rules(RULE), &mut fake),
        HookOutcome::Passthrough
    );
    assert!(matches!(
        evaluate_hooks_with_transport(&write_hook("fn broken("), &rules(RULE), &mut fake),
        HookOutcome::AllowPreToolUseWithContext { .. }
    ));
    assert!(fake.requests.is_empty());
}

#[test]
fn errors_and_expired_deadlines_remain_incomplete() {
    let hooks = write_hook("fn ada() {\n// Call run\nrun();\n}");
    for error in [
        EvaluationError::MissingCredential,
        EvaluationError::Http(401),
        EvaluationError::Http(429),
        EvaluationError::Http(500),
        EvaluationError::Timeout,
        EvaluationError::InvalidResponse,
    ] {
        let mut fake = Fake {
            error: Some(error),
            ..Fake::default()
        };
        let diagnostics =
            Plan::from_hooks(&hooks, &rules(RULE)).execute(&mut fake, Instant::now() + HOOK_BUDGET);
        pretty_assert_eq!(diagnostics[0].status, Status::Incomplete);
        pretty_assert_eq!(fake.requests.len(), 1);
    }
    let mut fake = Fake::default();
    let diagnostics = Plan::from_hooks(&hooks, &rules(RULE))
        .execute(&mut fake, Instant::now() - Duration::from_secs(1));
    assert!(fake.requests.is_empty());
    pretty_assert_eq!(diagnostics[0].status, Status::Incomplete);
}

#[test]
fn validates_exact_answer_set_type_probability_and_model() {
    let request = json!({"questions":{"q0":{},"q1":{}}});
    let good = json!({"model":client::MODEL,"answers":{"q1":{"type":"noul","noul":0.9},"q0":{"type":"noul","noul":0.1}}});
    pretty_assert_eq!(
        client::probabilities(&request, &good).expect("valid"),
        vec![0.1, 0.9]
    );
    for (pointer, value) in [
        ("/model", json!("jev-latest")),
        ("/answers/q0/type", json!("choice")),
        ("/answers/q0/noul", json!(1.1)),
        ("/answers/q0/noul", json!(null)),
    ] {
        let mut response = good.clone();
        *response.pointer_mut(pointer).expect("field") = value;
        assert!(client::probabilities(&request, &response).is_err());
    }
    let mut response = good;
    response["answers"]
        .as_object_mut()
        .expect("answers")
        .remove("q1");
    assert!(client::probabilities(&request, &response).is_err());
}

#[test]
fn replacements_and_deletions_map_exact_changed_ranges() {
    let dir = TempDir::new().expect("temp");
    fs::write(dir.path().join("src.rs"), "fn ada() {\nrun();\nrun();\n}\n").expect("write");
    let input = json!({"old_string":"run();", "new_string":"stop();"});
    assert!(edit::replacement(dir.path(), Path::new("src.rs"), &input).is_none());
    let input = json!({"old_string":"run();", "new_string":"stop();", "replace_all":true});
    let snapshot = edit::replacement(dir.path(), Path::new("src.rs"), &input).expect("all matches");
    pretty_assert_eq!(snapshot.content.matches("stop();").count(), 2);
    let deletion = edit::EditSnapshot::between(
        "fn ada() {\nold();\nrun();\n}\n",
        String::from("fn ada() {\nrun();\n}\n"),
    );
    assert!(deletion.changed.iter().any(Range::is_empty));
}

#[test]
fn unrelated_old_comments_are_not_evaluated_on_edit_for_all_adapters() {
    let dir = TempDir::new().expect("temp");
    let before = "fn ada() {\n// Call old\nold();\n}\nfn grace() {\nrun();\n}\n";
    fs::write(dir.path().join("src.rs"), before).expect("write");
    let yaml = RULE.replace("tool: Write", "tool: Edit");
    for parse in [claude::parse_hook, grok::parse_hook, cursor::parse_hook] {
        let hooks = parse(
            json!({"hook_event_name":"PreToolUse","tool_name":"Edit","cwd":dir.path(),
            "tool_input":{"file_path":"src.rs","old_string":"run();","new_string":"stop();"}}),
        )
        .expect("parse");
        let mut fake = Fake::default();
        assert!(
            Plan::from_hooks(&hooks, &rules(&yaml))
                .execute(&mut fake, Instant::now() + HOOK_BUDGET)
                .is_empty()
        );
        assert!(fake.requests.is_empty());
    }
    let hooks = codex::parse_hook(json!({"hook_event_name":"PreToolUse","tool_name":"apply_patch","cwd":dir.path(),
        "tool_input":{"command":"*** Begin Patch\n*** Update File: src.rs\n@@\n-run();\n+stop();\n*** End Patch"}})).expect("parse");
    let mut fake = Fake::default();
    assert!(
        Plan::from_hooks(&hooks, &rules(&yaml))
            .execute(&mut fake, Instant::now() + HOOK_BUDGET)
            .is_empty()
    );
    assert!(fake.requests.is_empty());
    assert!(
        matches!(&hooks[0], NudgeHook::PreToolUse(p) if matches!(&p.tool, ToolUse::Edit(e) if e.semantic_snapshot.is_some()))
    );
}

#[test]
fn changed_code_and_sequential_edits_use_final_snapshot() {
    let dir = TempDir::new().expect("temp");
    fs::write(
        dir.path().join("src.rs"),
        "fn ada() {\n// Call old\nold();\n}\n",
    )
    .expect("write");
    let yaml = RULE.replace("tool: Write", "tool: Edit");
    for parse in [claude::parse_hook, grok::parse_hook, cursor::parse_hook] {
        let hooks = parse(
            json!({"hook_event_name":"PreToolUse","tool_name":"MultiEdit","cwd":dir.path(),
            "tool_input":{"file_path":"src.rs","edits":[
                {"old_string":"old();","new_string":"intermediate();"},
                {"old_string":"intermediate();","new_string":"final_call();"}
            ]}}),
        )
        .expect("parse multi edit");
        let mut fake = Fake::default();
        Plan::from_hooks(&hooks, &rules(&yaml)).execute(&mut fake, Instant::now() + HOOK_BUDGET);
        pretty_assert_eq!(fake.requests.len(), 1);
        pretty_assert_eq!(
            fake.requests[0]["state"]["candidates"][0]["code"],
            "final_call();"
        );
        assert!(!fake.requests[0].to_string().contains("intermediate()"));
    }
}

#[test]
fn markdown_spans_and_duplicate_write_edit_rules_are_stable() {
    let yaml = RULE.replace(
        "file: '**/*.rs'",
        "file: '**/*.md'\n        target: {kind: MarkdownCodeBlock, language: rust}",
    );
    let rules = rules(&yaml);
    let rule = &rules[0];
    let matcher = rule.hooks_pretooluse_write().next().expect("matcher");
    let source = "# Ada\n\n```rust\nfn ada() {\n// Call run\nrun();\n}\n```\n";
    let mut plan = Plan::default();
    for _ in 0..2 {
        plan.add_file(
            Path::new("readme.md"),
            source,
            None,
            rule,
            &matcher.target,
            &matcher.content,
            matcher.semantic.as_ref().expect("semantic"),
        );
    }
    let mut fake = Fake {
        probability: 0.99,
        ..Fake::default()
    };
    let diagnostics = plan.execute(&mut fake, Instant::now() + HOOK_BUDGET);
    pretty_assert_eq!(diagnostics.len(), 1);
    pretty_assert_eq!(diagnostics[0].line, 5);
}

#[test]
fn oversized_candidates_are_incomplete_without_network() {
    let code = format!("fn ada() {{\n// {}\nrun();\n}}", "x".repeat(40_000));
    let mut fake = Fake::default();
    let diagnostics = Plan::from_hooks(&write_hook(&code), &rules(RULE))
        .execute(&mut fake, Instant::now() + HOOK_BUDGET);
    pretty_assert_eq!(diagnostics[0].status, Status::Incomplete);
    assert!(fake.requests.is_empty());
}

#[test]
fn deterministic_denials_skip_jev_and_warnings_preserve_substitutions() {
    let mut loaded = rules(RULE);
    loaded.extend(rules(
        r#"
version: 1
rules:
  - name: no-run
    message: Do not call run.
    on:
      - hook: PreToolUse
        tool: Write
        content: [{kind: Regex, pattern: 'run'}]
"#,
    ));
    let mut fake = Fake::default();
    assert!(matches!(
        evaluate_hooks_with_transport(
            &write_hook("fn ada() {\n// run\nrun();\n}"),
            &loaded,
            &mut fake
        ),
        HookOutcome::DenyPreToolUse { .. }
    ));
    assert!(fake.requests.is_empty());
    let loaded = rules(
        r#"
version: 1
rules:
  - name: use-pnpm
    action: substitute
    on:
      - hook: PreToolUse
        tool: Bash
        command: [{kind: Regex, pattern: '^npm install$', replace: 'pnpm install'}]
"#,
    );
    let mut hooks = claude::parse_hook(json!({"hook_event_name":"PreToolUse","tool_name":"Bash","cwd":"/tmp","tool_input":{"command":"npm install"}})).expect("bash hook");
    hooks.push(NudgeHook::WarnPreToolUse {
        message: String::from("inspection incomplete"),
    });
    match evaluate_hooks_with_transport(&hooks, &loaded, &mut fake) {
        HookOutcome::UpdatePreToolUse {
            updated_input,
            additional_context,
            ..
        } => {
            pretty_assert_eq!(updated_input["command"], "pnpm install");
            assert!(additional_context.contains("inspection incomplete"));
        }
        other => panic!("lost substitution: {other:?}"),
    }
}
