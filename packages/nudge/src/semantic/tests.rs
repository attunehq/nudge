use std::{
    fs,
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
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
    requests: Mutex<Vec<Value>>,
    /// Returned in order before falling back to `error` or success.
    transient: Mutex<Vec<EvaluationError>>,
    error: Option<EvaluationError>,
}

impl Fake {
    fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("requests").clone()
    }
}

impl Transport for Fake {
    fn send(&self, request: &Value, _: Instant) -> Result<Value, EvaluationError> {
        self.requests
            .lock()
            .expect("requests")
            .push(request.clone());
        let mut transient = self.transient.lock().expect("transient");
        if !transient.is_empty() {
            return Err(transient.remove(0));
        }
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
    assert!(select::comments("// standalone").expect("parse").is_empty());
}

#[test]
fn comments_without_following_code_use_nearest_code_in_scope() {
    let code = "// Section\n/// Docs\nfn ada(raw: &str) {\n    let x = parse(&raw);\n    // TODO: handle overflow\n}\n\nfn grace() {\n    // intentionally empty\n}\n";
    let candidates = select::comments(code).expect("candidates");
    let pairs = candidates
        .iter()
        .map(|c| {
            (
                c.state["comment"].as_str().expect("comment"),
                c.state["code"].as_str().expect("code"),
            )
        })
        .collect::<Vec<_>>();
    pretty_assert_eq!(
        pairs,
        vec![
            (
                "// Section",
                "fn ada(raw: &str) {\n    let x = parse(&raw);\n    // TODO: handle overflow\n}"
            ),
            ("// TODO: handle overflow", "let x = parse(&raw);"),
            ("// intentionally empty", "{\n    // intentionally empty\n}"),
        ]
    );
}

/// Answers each question from its candidate's comment so batch boundaries
/// cannot hide misattributed results. `FLAG` violates, `BROKEN` fails the
/// whole request, and `DROP` omits only that answer.
#[derive(Default)]
struct Keyed {
    delay: Duration,
    requests: AtomicUsize,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
    failed_questions: AtomicUsize,
}

impl Transport for Keyed {
    fn send(&self, request: &Value, _: Instant) -> Result<Value, EvaluationError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(current, Ordering::SeqCst);
        thread::sleep(self.delay);
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let candidates = request["state"]["candidates"]
            .as_array()
            .expect("candidates");
        let comment = |index: usize| candidates[index]["comment"].as_str().expect("comment");
        let questions = request["questions"].as_object().expect("questions");
        if (0..candidates.len()).any(|index| comment(index).contains("BROKEN")) {
            self.failed_questions
                .fetch_add(questions.len(), Ordering::SeqCst);
            return Err(EvaluationError::Http(400));
        }
        let answers = questions
            .iter()
            .filter_map(|(id, question)| {
                let instructions = question["instructions"].as_str().expect("instructions");
                let index = instructions["Evaluate only candidates[".len()..]
                    .split(']')
                    .next()
                    .and_then(|index| index.parse::<usize>().ok())
                    .expect("candidate index");
                let probability = if comment(index).contains("FLAG") {
                    0.99
                } else {
                    0.0
                };
                (!comment(index).contains("DROP"))
                    .then(|| (id.clone(), json!({"type":"noul", "noul": probability})))
            })
            .collect::<serde_json::Map<_, _>>();
        Ok(json!({"model":client::MODEL,"answers":answers}))
    }
}

/// Twenty-four padded comments, a few per request. Returns each comment's line.
fn many_batches(
    tag: impl Fn(usize) -> &'static str,
) -> (Vec<NudgeHook>, Vec<(usize, &'static str)>) {
    let padding = "x".repeat(8_000);
    let mut code = String::new();
    let mut lines = Vec::new();
    for index in 0..24 {
        lines.push((code.matches('\n').count() + 2, tag(index)));
        let tag = tag(index);
        code.push_str(&format!(
            "fn f{index}() {{\n// {tag} {index} {padding}\nrun();\n}}\n"
        ));
    }
    (write_hook(&code), lines)
}

fn every_third_flagged(index: usize) -> &'static str {
    if index.is_multiple_of(3) {
        "FLAG"
    } else {
        "keep"
    }
}

#[test]
fn concurrent_batches_attribute_results_in_job_order() {
    let (hooks, lines) = many_batches(every_third_flagged);
    let flagged = lines
        .iter()
        .filter(|(_, tag)| *tag == "FLAG")
        .map(|(line, _)| *line)
        .collect::<Vec<_>>();
    let transport = Keyed {
        delay: Duration::from_millis(50),
        ..Keyed::default()
    };
    let diagnostics = Plan::from_hooks(&hooks, &rules(RULE))
        .execute(&transport, Instant::now() + Duration::from_secs(30));
    assert!(transport.requests.load(Ordering::SeqCst) > CONCURRENT_REQUESTS);
    assert!(transport.max_in_flight.load(Ordering::SeqCst) > 1);
    assert!(transport.max_in_flight.load(Ordering::SeqCst) <= CONCURRENT_REQUESTS);
    assert!(diagnostics.iter().all(|d| d.status == Status::Finding));
    pretty_assert_eq!(
        diagnostics.iter().map(|d| d.line).collect::<Vec<_>>(),
        flagged
    );
}

#[test]
fn failures_skip_only_their_own_batch_or_answer() {
    let (hooks, lines) = many_batches(|index| match index {
        4 => "BROKEN",
        19 => "DROP",
        index => every_third_flagged(index),
    });
    let transport = Keyed::default();
    let diagnostics = Plan::from_hooks(&hooks, &rules(RULE))
        .execute(&transport, Instant::now() + Duration::from_secs(30));
    let skipped = diagnostics
        .iter()
        .filter(|d| d.status == Status::Skipped)
        .map(|d| (d.line, d.render()))
        .collect::<Vec<_>>();
    let line = |index: usize| lines[index].0;
    assert!(skipped.iter().any(|(l, message)| *l == line(4)
        && message.contains("comment not evaluated: Jev returned HTTP 400")));
    assert!(skipped.iter().any(|(l, message)| *l == line(19)
        && message.contains("comment not evaluated: Jev returned an invalid response")));
    // Only the failed request's comments and the dropped answer are skipped.
    let failed = transport.failed_questions.load(Ordering::SeqCst);
    assert!(failed < 24 / 2);
    pretty_assert_eq!(skipped.len(), failed + 1);
    for (line, tag) in lines {
        if tag == "FLAG" && skipped.iter().all(|(l, _)| *l != line) {
            assert!(
                diagnostics
                    .iter()
                    .any(|d| d.line == line && d.status == Status::Finding)
            );
        }
    }
}

#[test]
fn retryable_errors_back_off_and_recover() {
    let hooks = write_hook("fn ada() {\n// Call run\nrun();\n}");
    let fake = Fake {
        probability: 0.99,
        transient: Mutex::new(vec![EvaluationError::Http(529), EvaluationError::Http(429)]),
        ..Fake::default()
    };
    let started = Instant::now();
    let diagnostics = Plan::from_hooks(&hooks, &rules(RULE))
        .execute(&fake, Instant::now() + Duration::from_secs(30));
    pretty_assert_eq!(fake.requests().len(), 3);
    assert!(started.elapsed() >= (RETRY_DELAYS[0] + RETRY_DELAYS[1]) / 2);
    pretty_assert_eq!(diagnostics[0].status, Status::Finding);
}

#[test]
fn exhausted_and_permanent_failures_warn_about_the_skipped_comment() {
    let hooks = write_hook("fn ada() {\n// Call run\nrun();\n}");
    for (error, attempts) in [
        (EvaluationError::Unavailable, RETRY_DELAYS.len() + 1),
        (EvaluationError::Http(401), 1),
        (EvaluationError::MissingCredential, 1),
        (EvaluationError::InvalidResponse, 1),
    ] {
        let fake = Fake {
            error: Some(error.clone()),
            ..Fake::default()
        };
        let diagnostics = Plan::from_hooks(&hooks, &rules(RULE))
            .execute(&fake, Instant::now() + Duration::from_secs(30));
        pretty_assert_eq!(fake.requests().len(), attempts);
        pretty_assert_eq!(diagnostics.len(), 1);
        pretty_assert_eq!(diagnostics[0].status, Status::Skipped);
        let rendered = diagnostics[0].render();
        assert!(rendered.starts_with(
            "src.rs:2 [explain-intent] semantic check skipped (warning): comment not evaluated: "
        ));
        assert!(rendered.contains(&error.to_string()));
        pretty_assert_eq!(
            rendered.contains(&format!("gave up after {attempts} attempts")),
            attempts > 1
        );
    }
}

#[test]
fn warning_policy_and_batching_preserve_distinct_predicates() {
    let hooks = write_hook("fn grace() {\n// Call run\nrun();\n// Call stop\nstop();\n}\n");
    for (probability, status) in [
        (0.1, None),
        (0.5, Some(Status::Uncertain)),
        (0.9, Some(Status::Finding)),
    ] {
        let fake = Fake {
            probability,
            ..Fake::default()
        };
        let diagnostics =
            Plan::from_hooks(&hooks, &rules(RULE)).execute(&fake, Instant::now() + HOOK_BUDGET);
        pretty_assert_eq!(fake.requests().len(), 1);
        pretty_assert_eq!(
            fake.requests()[0]["questions"]
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
    let fake = Fake {
        probability: 0.99,
        ..Fake::default()
    };
    assert!(matches!(
        evaluate_hooks_with_transport(&hooks, &rules(RULE), &fake),
        HookOutcome::AllowPreToolUseWithContext { .. }
    ));
}

#[test]
fn semantic_severity_and_custom_thresholds_control_hook_decisions() {
    let hooks = write_hook("fn grace() {\n// Call run\nrun();\n}\n");
    for action in ["warn", "block"] {
        let yaml = RULE
            .replace("action: warn", &format!("action: {action}"))
            .replace("violation: 0.9", "violation: 0.8")
            .replace(
                "        semantic:",
                "        content: [{kind: Regex, pattern: run}]\n        semantic:",
            );
        for probability in [0.0, 0.1, 0.1001, 0.7999, 0.8, 1.0] {
            let fake = Fake {
                probability,
                ..Fake::default()
            };
            let outcome = evaluate_hooks_with_transport(&hooks, &rules(&yaml), &fake);
            pretty_assert_eq!(fake.requests().len(), 1);
            if probability <= 0.1 {
                pretty_assert_eq!(outcome, HookOutcome::Passthrough);
            } else if probability >= 0.8 && action == "block" {
                assert!(matches!(outcome, HookOutcome::DenyPreToolUse { .. }));
            } else {
                assert!(matches!(
                    outcome,
                    HookOutcome::AllowPreToolUseWithContext { .. }
                ));
            }
        }
        for error in [
            EvaluationError::MissingCredential,
            EvaluationError::InvalidCredentialFile,
            EvaluationError::Http(401),
            EvaluationError::Timeout,
        ] {
            let fake = Fake {
                error: Some(error),
                ..Fake::default()
            };
            assert!(matches!(
                evaluate_hooks_with_transport(&hooks, &rules(&yaml), &fake),
                HookOutcome::AllowPreToolUseWithContext { .. }
            ));
        }
    }
}

#[test]
fn semantic_edit_preconditions_never_block_without_a_model_finding() {
    let dir = TempDir::new().expect("temp");
    fs::write(
        dir.path().join("src.rs"),
        "fn ada() {\n// Call run\nrun();\n}\n",
    )
    .expect("source");
    let yaml = RULE
        .replace("tool: Write", "tool: Edit")
        .replace("action: warn", "action: block")
        .replace(
            "        semantic:",
            "        new_content: [{kind: Regex, pattern: stop}]\n        semantic:",
        );
    let hooks = claude::parse_hook(
        json!({"hook_event_name":"PreToolUse", "tool_name":"Edit", "cwd":dir.path(),
        "tool_input":{"file_path":"src.rs", "old_string":"run();", "new_string":"stop();"}}),
    )
    .expect("edit");
    for probability in [0.0, 0.9] {
        let fake = Fake {
            probability,
            ..Fake::default()
        };
        let outcome = evaluate_hooks_with_transport(&hooks, &rules(&yaml), &fake);
        pretty_assert_eq!(fake.requests().len(), 1);
        pretty_assert_eq!(
            matches!(outcome, HookOutcome::DenyPreToolUse { .. }),
            probability >= 0.9
        );
    }
}

#[test]
fn mixed_warning_and_block_rules_keep_their_own_actions() {
    let mut loaded = rules(RULE);
    loaded.extend(rules(
        &RULE
            .replace("action: warn", "action: block")
            .replace("name: explain-intent", "name: error-intent"),
    ));
    let hooks = write_hook("fn grace() {\n// Call run\nrun();\n}\n");
    let fake = Fake {
        probability: 0.9,
        ..Fake::default()
    };
    let diagnostics =
        Plan::from_hooks(&hooks, &loaded).execute(&fake, Instant::now() + HOOK_BUDGET);
    pretty_assert_eq!(diagnostics.len(), 2);
    assert!(!diagnostics[0].is_error());
    assert!(diagnostics[1].is_error());
    assert!(matches!(
        evaluate_hooks_with_transport(&hooks, &loaded, &fake),
        HookOutcome::DenyPreToolUse { .. }
    ));
}

#[test]
fn missing_context_preconditions_and_no_candidates_do_not_call_jev() {
    let fake = Fake::default();
    let precondition = RULE.replace(
        "        semantic:",
        "        content:\n          - kind: Regex\n            pattern: NEVER\n        semantic:",
    );
    let hooks = write_hook("fn ada() {\n// Call run\nrun();\n}");
    pretty_assert_eq!(
        evaluate_hooks_with_transport(&hooks, &rules(&precondition), &fake),
        HookOutcome::Passthrough
    );
    pretty_assert_eq!(
        evaluate_hooks_with_transport(&write_hook("fn ada() {}"), &rules(RULE), &fake),
        HookOutcome::Passthrough
    );
    assert!(matches!(
        evaluate_hooks_with_transport(&write_hook("fn broken("), &rules(RULE), &fake),
        HookOutcome::AllowPreToolUseWithContext { .. }
    ));
    assert!(fake.requests().is_empty());
}

#[test]
fn errors_and_expired_deadlines_are_skipped_within_the_hook_budget() {
    let hooks = write_hook("fn ada() {\n// Call run\nrun();\n}");
    for error in [
        EvaluationError::MissingCredential,
        EvaluationError::Http(401),
        EvaluationError::Http(429),
        EvaluationError::Http(500),
        EvaluationError::Timeout,
        EvaluationError::InvalidResponse,
    ] {
        let retryable = error.is_retryable();
        let fake = Fake {
            error: Some(error),
            ..Fake::default()
        };
        let started = Instant::now();
        let diagnostics =
            Plan::from_hooks(&hooks, &rules(RULE)).execute(&fake, Instant::now() + HOOK_BUDGET);
        assert!(started.elapsed() < HOOK_BUDGET);
        pretty_assert_eq!(diagnostics[0].status, Status::Skipped);
        pretty_assert_eq!(fake.requests().len() > 1, retryable);
    }
    let fake = Fake::default();
    let diagnostics = Plan::from_hooks(&hooks, &rules(RULE))
        .execute(&fake, Instant::now() - Duration::from_secs(1));
    assert!(fake.requests().is_empty());
    pretty_assert_eq!(diagnostics[0].status, Status::Skipped);
}

#[test]
fn validates_model_per_response_and_answers_per_question() {
    let request = json!({"questions":{"q0":{},"q1":{}}});
    let good = json!({"model":client::MODEL,"answers":{"q1":{"type":"noul","noul":0.9},"q0":{"type":"noul","noul":0.1}}});
    pretty_assert_eq!(
        client::probabilities(&request, &good).expect("valid"),
        vec![Ok(0.1), Ok(0.9)]
    );
    for (pointer, value) in [("/model", json!("jev-latest")), ("/answers", json!([]))] {
        let mut response = good.clone();
        *response.pointer_mut(pointer).expect("field") = value;
        assert!(client::probabilities(&request, &response).is_err());
    }
    for (pointer, value) in [
        ("/answers/q0/type", json!("choice")),
        ("/answers/q0/noul", json!(1.1)),
        ("/answers/q0/noul", json!(null)),
    ] {
        let mut response = good.clone();
        *response.pointer_mut(pointer).expect("field") = value;
        pretty_assert_eq!(
            client::probabilities(&request, &response).expect("response"),
            vec![Err(EvaluationError::InvalidResponse), Ok(0.9)]
        );
    }
    let mut response = good;
    response["answers"]
        .as_object_mut()
        .expect("answers")
        .remove("q1");
    pretty_assert_eq!(
        client::probabilities(&request, &response).expect("response"),
        vec![Ok(0.1), Err(EvaluationError::InvalidResponse)]
    );
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
        let fake = Fake::default();
        assert!(
            Plan::from_hooks(&hooks, &rules(&yaml))
                .execute(&fake, Instant::now() + HOOK_BUDGET)
                .is_empty()
        );
        assert!(fake.requests().is_empty());
    }
    let hooks = codex::parse_hook(json!({"hook_event_name":"PreToolUse","tool_name":"apply_patch","cwd":dir.path(),
        "tool_input":{"command":"*** Begin Patch\n*** Update File: src.rs\n@@\n-run();\n+stop();\n*** End Patch"}})).expect("parse");
    let fake = Fake::default();
    assert!(
        Plan::from_hooks(&hooks, &rules(&yaml))
            .execute(&fake, Instant::now() + HOOK_BUDGET)
            .is_empty()
    );
    assert!(fake.requests().is_empty());
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
        let fake = Fake::default();
        Plan::from_hooks(&hooks, &rules(&yaml)).execute(&fake, Instant::now() + HOOK_BUDGET);
        pretty_assert_eq!(fake.requests().len(), 1);
        pretty_assert_eq!(
            fake.requests()[0]["state"]["candidates"][0]["code"],
            "final_call();"
        );
        assert!(!fake.requests()[0].to_string().contains("intermediate()"));
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
    let fake = Fake {
        probability: 0.99,
        ..Fake::default()
    };
    let diagnostics = plan.execute(&fake, Instant::now() + HOOK_BUDGET);
    pretty_assert_eq!(diagnostics.len(), 1);
    pretty_assert_eq!(diagnostics[0].line, 5);
}

#[test]
fn oversized_candidates_are_incomplete_without_network() {
    let code = format!("fn ada() {{\n// {}\nrun();\n}}", "x".repeat(40_000));
    let fake = Fake::default();
    let diagnostics = Plan::from_hooks(&write_hook(&code), &rules(RULE))
        .execute(&fake, Instant::now() + HOOK_BUDGET);
    pretty_assert_eq!(diagnostics[0].status, Status::Skipped);
    assert!(fake.requests().is_empty());
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
    let fake = Fake::default();
    assert!(matches!(
        evaluate_hooks_with_transport(&write_hook("fn ada() {\n// run\nrun();\n}"), &loaded, &fake),
        HookOutcome::DenyPreToolUse { .. }
    ));
    assert!(fake.requests().is_empty());
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
    match evaluate_hooks_with_transport(&hooks, &loaded, &fake) {
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
