//! Provider-specific hook response rendering.

use color_eyre::eyre::{Context, Result};
use serde::Serialize;

use crate::agent::AgentKind;

/// Agent-neutral response decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    /// Exit successfully with no output.
    Passthrough,

    /// Deny a `PreToolUse` operation.
    DenyPreToolUse {
        /// Feedback shown to the agent and user.
        message: String,
    },

    /// Add context for `UserPromptSubmit`.
    AddContext {
        /// Context text.
        context: String,
    },
}

/// Render and print a hook outcome.
pub fn emit(agent: AgentKind, outcome: HookOutcome) -> Result<()> {
    match render(agent, outcome)? {
        RenderedHookOutcome::NoOutput => {}
        RenderedHookOutcome::Stdout(output) => println!("{output}"),
    }

    Ok(())
}

/// Render a hook outcome without printing.
pub fn render(_agent: AgentKind, outcome: HookOutcome) -> Result<RenderedHookOutcome> {
    match outcome {
        HookOutcome::Passthrough => Ok(RenderedHookOutcome::NoOutput),
        HookOutcome::AddContext { context } => Ok(RenderedHookOutcome::Stdout(context)),
        HookOutcome::DenyPreToolUse { message } => {
            let response = PreToolUseDenyResponse {
                system_message: "Nudge blocked operation due to rule violation.".to_string(),
                hook_specific_output: PreToolUseDenyOutput {
                    hook_event_name: "PreToolUse".to_string(),
                    permission_decision: "deny".to_string(),
                    permission_decision_reason: message,
                },
            };

            Ok(RenderedHookOutcome::Stdout(
                serde_json::to_string(&response).context("serialize hook response")?,
            ))
        }
    }
}

/// Rendered hook output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderedHookOutcome {
    /// No output should be emitted.
    NoOutput,

    /// Text should be emitted on stdout.
    Stdout(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreToolUseDenyResponse {
    system_message: String,
    hook_specific_output: PreToolUseDenyOutput,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreToolUseDenyOutput {
    hook_event_name: String,
    permission_decision: String,
    permission_decision_reason: String,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq as pretty_assert_eq;
    use serde_json::Value;

    use crate::{
        agent::AgentKind,
        hook::response::{HookOutcome, RenderedHookOutcome, render},
    };

    #[test]
    fn claude_denial_json_contains_permission_decision() {
        let rendered = render(
            AgentKind::Claude,
            HookOutcome::DenyPreToolUse {
                message: "blocked".to_string(),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            Value::String("deny".to_string())
        );
    }

    #[test]
    fn codex_denial_json_omits_unsupported_fields() {
        let rendered = render(
            AgentKind::Codex,
            HookOutcome::DenyPreToolUse {
                message: "blocked".to_string(),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            Value::String("deny".to_string())
        );
        assert!(json.get("continue").is_none());
        assert!(json.get("stopReason").is_none());
        assert!(json.get("suppressOutput").is_none());
    }

    #[test]
    fn permission_request_passthrough_renders_no_output() {
        let rendered = render(AgentKind::Claude, HookOutcome::Passthrough).expect("render");
        pretty_assert_eq!(rendered, RenderedHookOutcome::NoOutput);

        let rendered = render(AgentKind::Codex, HookOutcome::Passthrough).expect("render");
        pretty_assert_eq!(rendered, RenderedHookOutcome::NoOutput);
    }

    #[test]
    fn user_prompt_context_renders_plain_text() {
        let rendered = render(
            AgentKind::Codex,
            HookOutcome::AddContext {
                context: "remember this".to_string(),
            },
        )
        .expect("render");

        pretty_assert_eq!(
            rendered,
            RenderedHookOutcome::Stdout("remember this".to_string())
        );
    }
}
