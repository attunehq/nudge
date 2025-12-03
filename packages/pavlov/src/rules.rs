//! Rule evaluation for Claude Code hooks.
//!
//! Rules are loaded from YAML configuration files and evaluated against hooks.
//! All matching rules fire, and their messages are concatenated.

pub mod config;
pub mod eval;
pub mod schema;

use std::collections::HashSet;

use color_eyre::eyre::Result;

use crate::claude::hook::{
    ContinueResponse, Hook, InterruptResponse, PreToolUseOutput, Response,
};

use self::config::load_all_rules;
use self::eval::CompiledRule;

/// Registry of all loaded rules.
pub struct RuleRegistry {
    rules: Vec<CompiledRule>,
}

impl RuleRegistry {
    /// Create a new registry by loading rules from all sources.
    ///
    /// Loading order (all additive):
    /// 1. `~/.config/pavlov/rules.yaml` if it exists
    /// 2. `.pavlov.yaml` if it exists
    /// 3. `.pavlov/` directory walked recursively (sorted), loading all `*.yaml` files
    pub fn new() -> Result<Self> {
        let mut rules = vec![];
        let mut seen_names = HashSet::new();

        let all_rules = load_all_rules()?;

        for rule in all_rules {
            // Warn on duplicate names (but still add the rule)
            if !seen_names.insert(rule.name.clone()) {
                tracing::warn!("Multiple rules with name '{}' found", rule.name);
            }
            let compiled = CompiledRule::compile(rule)?;
            rules.push(compiled);
        }

        Ok(Self { rules })
    }

    /// Evaluate all rules against a hook.
    ///
    /// All matching rules fire. Messages are concatenated with `\n\n---\n\n`.
    /// If any rule returns `interrupt`, the overall response is `interrupt`.
    pub fn evaluate(&self, hook: &Hook) -> Response {
        let mut messages = vec![];
        let mut any_interrupt = false;

        for rule in &self.rules {
            if let Some(result) = rule.evaluate(hook) {
                messages.push(result.message);
                if result.is_interrupt {
                    any_interrupt = true;
                }
            }
        }

        if messages.is_empty() {
            return Response::Passthrough;
        }

        let combined_message = messages.join("\n\n---\n\n");

        if any_interrupt {
            Response::Interrupt(
                InterruptResponse::builder()
                    .stop_reason("Rule violation detected")
                    .system_message(combined_message)
                    .hook_specific_output(
                        serde_json::to_value(PreToolUseOutput::default()).unwrap(),
                    )
                    .build(),
            )
        } else {
            Response::Continue(
                ContinueResponse::builder()
                    .system_message(combined_message)
                    .hook_specific_output(
                        serde_json::to_value(PreToolUseOutput::default()).unwrap(),
                    )
                    .build(),
            )
        }
    }

    /// Get the number of loaded rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Check if no rules are loaded.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Evaluate all rules against a hook.
///
/// This is a convenience function that creates a RuleRegistry and evaluates the hook.
/// For repeated evaluations, prefer creating a RuleRegistry once and reusing it.
pub fn evaluate_all(hook: &Hook) -> Response {
    match RuleRegistry::new() {
        Ok(registry) => registry.evaluate(hook),
        Err(e) => {
            tracing::error!("Failed to load rules: {e}");
            Response::Passthrough
        }
    }
}
