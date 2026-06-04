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

    /// Allow a `PreToolUse` operation while surfacing warning context.
    AllowPreToolUseWithContext {
        /// User-visible audit message.
        system_message: String,

        /// Model-visible context explaining the warning.
        additional_context: String,
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
            let response = PreToolUseResponse {
                system_message: Some(String::from(
                    "Nudge blocked operation due to rule violation.",
                )),
                hook_specific_output: PreToolUseOutput {
                    hook_event_name: String::from("PreToolUse"),
                    permission_decision: String::from("deny"),
                    permission_decision_reason: Some(message),
                    updated_input: None,
                    additional_context: None,
                },
            };

            Ok(RenderedHookOutcome::Stdout(
                serde_json::to_string(&response).context("serialize hook response")?,
            ))
        }
        HookOutcome::AllowPreToolUseWithContext {
            system_message,
            additional_context,
        } => {
            let response = PreToolUseResponse {
                system_message: Some(system_message),
                hook_specific_output: PreToolUseOutput {
                    hook_event_name: String::from("PreToolUse"),
                    permission_decision: String::from("allow"),
                    permission_decision_reason: None,
                    updated_input: None,
                    additional_context: Some(additional_context),
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
                    hook_event_name: String::from("PreToolUse"),
                    permission_decision: String::from("allow"),
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
                message: String::from("blocked"),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            Value::String(String::from("deny"))
        );
    }

    #[test]
    fn codex_denial_json_omits_unsupported_fields() {
        let rendered = render(
            AgentKind::Codex,
            HookOutcome::DenyPreToolUse {
                message: String::from("blocked"),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            Value::String(String::from("deny"))
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
                system_message: String::from(
                    "Nudge substituted `npm install foo` -> `yarn add foo`.",
                ),
                additional_context: String::from(
                    "Nudge rewrote the Bash command from `npm install foo` to `yarn add foo` before execution.",
                ),
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
            Value::String(String::from("allow"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["updatedInput"]["command"],
            Value::String(String::from("yarn add foo"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["updatedInput"]["description"],
            Value::String(String::from("Install foo"))
        );
        assert!(json["hookSpecificOutput"]["additionalContext"].is_string());
    }

    #[test]
    fn warning_response_allows_with_context_and_no_updated_input() {
        let rendered = render(
            AgentKind::Codex,
            HookOutcome::AllowPreToolUseWithContext {
                system_message: String::from("Nudge allowed the operation with a warning."),
                additional_context: String::from("Tell the user about this warning."),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            Value::String(String::from("allow"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["additionalContext"],
            Value::String(String::from("Tell the user about this warning."))
        );
        assert!(json["hookSpecificOutput"].get("updatedInput").is_none());
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
                context: String::from("remember this"),
            },
        )
        .expect("render");

        pretty_assert_eq!(
            rendered,
            RenderedHookOutcome::Stdout(String::from("remember this"))
        );
    }
}
