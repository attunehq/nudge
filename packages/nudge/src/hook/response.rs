//! Provider-specific hook response rendering.

use color_eyre::eyre::{Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::agent::AgentKind;

/// Agent-neutral response decision.
#[derive(Debug, Clone, PartialEq)]
pub enum HookOutcome {
    /// Exit successfully with no output.
    Passthrough,

    /// Deny a `PreToolUse` operation.
    DenyPreToolUse {
        /// Feedback shown to the agent and user.
        message: String,
    },

    /// Update a `PreToolUse` operation and allow it to proceed.
    UpdatePreToolUse {
        /// User-visible audit message.
        system_message: String,

        /// Model-visible context explaining the rewrite.
        additional_context: String,

        /// Full updated provider tool input.
        updated_input: Value,
    },

    /// Add context for `UserPromptSubmit`.
    AddContext {
        /// Context text.
        context: String,
    },

    /// Prevent a `Stop` event from ending the turn.
    ContinueStop {
        /// Feedback sent to the agent as its continuation prompt.
        reason: String,
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
        HookOutcome::ContinueStop { reason } => {
            let response = StopResponse {
                decision: "block".to_string(),
                reason,
            };

            Ok(RenderedHookOutcome::Stdout(
                serde_json::to_string(&response).context("serialize stop response")?,
            ))
        }
        HookOutcome::DenyPreToolUse { message } => {
            let response = PreToolUseResponse {
                system_message: Some("Nudge blocked operation due to rule violation.".to_string()),
                hook_specific_output: PreToolUseOutput {
                    hook_event_name: "PreToolUse".to_string(),
                    permission_decision: "deny".to_string(),
                    permission_decision_reason: Some(message),
                    updated_input: None,
                    additional_context: None,
                },
            };

            Ok(RenderedHookOutcome::Stdout(
                serde_json::to_string(&response).context("serialize hook response")?,
            ))
        }
        HookOutcome::UpdatePreToolUse {
            system_message,
            additional_context,
            updated_input,
        } => {
            let response = PreToolUseResponse {
                system_message: Some(system_message),
                hook_specific_output: PreToolUseOutput {
                    hook_event_name: "PreToolUse".to_string(),
                    permission_decision: "allow".to_string(),
                    permission_decision_reason: None,
                    updated_input: Some(updated_input),
                    additional_context: Some(additional_context),
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
struct PreToolUseResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_message: Option<String>,
    hook_specific_output: PreToolUseOutput,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreToolUseOutput {
    hook_event_name: String,
    permission_decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    permission_decision_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    additional_context: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StopResponse {
    decision: String,
    reason: String,
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
    fn substitution_response_allows_with_updated_input_and_context() {
        let rendered = render(
            AgentKind::Codex,
            HookOutcome::UpdatePreToolUse {
                system_message: "Nudge substituted `npm install foo` -> `yarn add foo`."
                    .to_string(),
                additional_context:
                    "Nudge rewrote the Bash command from `npm install foo` to `yarn add foo` before execution."
                        .to_string(),
                updated_input: serde_json::json!({
                    "command": "yarn add foo",
                    "description": "Install foo"
                }),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            Value::String("allow".to_string())
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["updatedInput"]["command"],
            Value::String("yarn add foo".to_string())
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["updatedInput"]["description"],
            Value::String("Install foo".to_string())
        );
        assert!(json["hookSpecificOutput"]["additionalContext"].is_string());
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

    #[test]
    fn stop_continuation_renders_block_decision() {
        let rendered = render(
            AgentKind::Codex,
            HookOutcome::ContinueStop {
                reason: "Run tests before stopping.".to_string(),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(json["decision"], Value::String("block".to_string()));
        pretty_assert_eq!(
            json["reason"],
            Value::String("Run tests before stopping.".to_string())
        );
    }
}
