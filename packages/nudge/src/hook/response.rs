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
pub fn render(agent: AgentKind, outcome: HookOutcome) -> Result<RenderedHookOutcome> {
    match agent {
        AgentKind::Cursor => render_cursor(outcome),
        AgentKind::Grok => render_grok(outcome),
        AgentKind::Claude | AgentKind::Codex => render_claude_compat(outcome),
    }
}

fn render_claude_compat(outcome: HookOutcome) -> Result<RenderedHookOutcome> {
    match outcome {
        HookOutcome::Passthrough => Ok(RenderedHookOutcome::NoOutput),
        HookOutcome::AddContext { context } => Ok(RenderedHookOutcome::Stdout(context)),
        HookOutcome::DenyPreToolUse { message } => serialize_pretooluse(PreToolUseResponse {
            decision: None,
            reason: None,
            system_message: Some(String::from(
                "Nudge blocked operation due to rule violation.",
            )),
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("deny")),
                permission_decision_reason: Some(message),
                updated_input: None,
                additional_context: None,
            },
        }),
        HookOutcome::AllowPreToolUseWithContext {
            system_message,
            additional_context,
        } => serialize_pretooluse(PreToolUseResponse {
            decision: None,
            reason: None,
            system_message: Some(system_message),
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("allow")),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: Some(additional_context),
            },
        }),
        HookOutcome::UpdatePreToolUse {
            system_message,
            additional_context,
            updated_input,
        } => serialize_pretooluse(PreToolUseResponse {
            decision: None,
            reason: None,
            system_message: Some(system_message),
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("allow")),
                permission_decision_reason: None,
                updated_input: Some(updated_input),
                additional_context: Some(additional_context),
            },
        }),
    }
}

fn render_cursor(outcome: HookOutcome) -> Result<RenderedHookOutcome> {
    match outcome {
        HookOutcome::Passthrough => Ok(RenderedHookOutcome::NoOutput),
        HookOutcome::AddContext { context } => serialize_cursor(CursorHookResponse {
            permission: None,
            user_message: None,
            agent_message: None,
            updated_input: None,
            continue_submission: Some(true),
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("UserPromptSubmit"),
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                additional_context: Some(context),
            },
        }),
        HookOutcome::DenyPreToolUse { message } => serialize_cursor(CursorHookResponse {
            permission: Some(String::from("deny")),
            user_message: Some(String::from(
                "Nudge blocked operation due to rule violation.",
            )),
            agent_message: Some(message.clone()),
            updated_input: None,
            continue_submission: None,
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("deny")),
                permission_decision_reason: Some(message),
                updated_input: None,
                additional_context: None,
            },
        }),
        HookOutcome::AllowPreToolUseWithContext {
            system_message,
            additional_context,
        } => serialize_cursor(CursorHookResponse {
            permission: Some(String::from("allow")),
            user_message: Some(system_message),
            agent_message: Some(additional_context.clone()),
            updated_input: None,
            continue_submission: None,
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("allow")),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: Some(additional_context),
            },
        }),
        HookOutcome::UpdatePreToolUse {
            system_message,
            additional_context,
            updated_input,
        } => serialize_cursor(CursorHookResponse {
            permission: Some(String::from("allow")),
            user_message: Some(system_message),
            agent_message: Some(additional_context.clone()),
            updated_input: Some(updated_input.clone()),
            continue_submission: None,
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("allow")),
                permission_decision_reason: None,
                updated_input: Some(updated_input),
                additional_context: Some(additional_context),
            },
        }),
    }
}

fn render_grok(outcome: HookOutcome) -> Result<RenderedHookOutcome> {
    match outcome {
        HookOutcome::Passthrough => Ok(RenderedHookOutcome::NoOutput),
        HookOutcome::AddContext { context } => serialize_pretooluse(PreToolUseResponse {
            decision: None,
            reason: None,
            system_message: None,
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("UserPromptSubmit"),
                permission_decision: None,
                permission_decision_reason: None,
                updated_input: None,
                additional_context: Some(context),
            },
        }),
        HookOutcome::DenyPreToolUse { message } => serialize_pretooluse(PreToolUseResponse {
            decision: Some(String::from("deny")),
            reason: Some(message.clone()),
            system_message: Some(String::from(
                "Nudge blocked operation due to rule violation.",
            )),
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("deny")),
                permission_decision_reason: Some(message),
                updated_input: None,
                additional_context: None,
            },
        }),
        HookOutcome::AllowPreToolUseWithContext {
            system_message,
            additional_context,
        } => serialize_pretooluse(PreToolUseResponse {
            decision: Some(String::from("allow")),
            reason: None,
            system_message: Some(system_message),
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("allow")),
                permission_decision_reason: None,
                updated_input: None,
                additional_context: Some(additional_context),
            },
        }),
        HookOutcome::UpdatePreToolUse {
            system_message,
            additional_context,
            updated_input,
        } => serialize_pretooluse(PreToolUseResponse {
            decision: Some(String::from("allow")),
            reason: None,
            system_message: Some(system_message),
            hook_specific_output: PreToolUseOutput {
                hook_event_name: String::from("PreToolUse"),
                permission_decision: Some(String::from("allow")),
                permission_decision_reason: None,
                updated_input: Some(updated_input),
                additional_context: Some(additional_context),
            },
        }),
    }
}

fn serialize_pretooluse(response: PreToolUseResponse) -> Result<RenderedHookOutcome> {
    Ok(RenderedHookOutcome::Stdout(
        serde_json::to_string(&response).context("serialize hook response")?,
    ))
}

fn serialize_cursor(response: CursorHookResponse) -> Result<RenderedHookOutcome> {
    Ok(RenderedHookOutcome::Stdout(
        serde_json::to_string(&response).context("serialize hook response")?,
    ))
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
    decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_message: Option<String>,
    hook_specific_output: PreToolUseOutput,
}

#[derive(Debug, Serialize)]
struct CursorHookResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    permission: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_input: Option<Value>,
    #[serde(rename = "continue", skip_serializing_if = "Option::is_none")]
    continue_submission: Option<bool>,
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: PreToolUseOutput,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreToolUseOutput {
    hook_event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    permission_decision: Option<String>,
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

    #[test]
    fn grok_denial_uses_native_decision_and_claude_compat_fields() {
        let rendered = render(
            AgentKind::Grok,
            HookOutcome::DenyPreToolUse {
                message: String::from("blocked"),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(json["decision"], Value::String(String::from("deny")));
        pretty_assert_eq!(json["reason"], Value::String(String::from("blocked")));
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            Value::String(String::from("deny"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecisionReason"],
            Value::String(String::from("blocked"))
        );
    }

    #[test]
    fn grok_substitution_allows_with_updated_input() {
        let rendered = render(
            AgentKind::Grok,
            HookOutcome::UpdatePreToolUse {
                system_message: String::from("Nudge substituted a command."),
                additional_context: String::from("rewrote npm to yarn"),
                updated_input: serde_json::json!({ "command": "yarn add foo" }),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(json["decision"], Value::String(String::from("allow")));
        pretty_assert_eq!(
            json["hookSpecificOutput"]["updatedInput"]["command"],
            Value::String(String::from("yarn add foo"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["additionalContext"],
            Value::String(String::from("rewrote npm to yarn"))
        );
    }

    #[test]
    fn grok_user_prompt_context_renders_additional_context_json() {
        let rendered = render(
            AgentKind::Grok,
            HookOutcome::AddContext {
                context: String::from("remember this"),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(
            json["hookSpecificOutput"]["hookEventName"],
            Value::String(String::from("UserPromptSubmit"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["additionalContext"],
            Value::String(String::from("remember this"))
        );
    }

    #[test]
    fn grok_passthrough_renders_no_output() {
        let rendered = render(AgentKind::Grok, HookOutcome::Passthrough).expect("render");
        pretty_assert_eq!(rendered, RenderedHookOutcome::NoOutput);
    }

    #[test]
    fn cursor_denial_uses_native_permission_and_claude_compat_fields() {
        let rendered = render(
            AgentKind::Cursor,
            HookOutcome::DenyPreToolUse {
                message: String::from("blocked"),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(json["permission"], Value::String(String::from("deny")));
        pretty_assert_eq!(
            json["agent_message"],
            Value::String(String::from("blocked"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecision"],
            Value::String(String::from("deny"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["permissionDecisionReason"],
            Value::String(String::from("blocked"))
        );
    }

    #[test]
    fn cursor_substitution_allows_with_updated_input() {
        let rendered = render(
            AgentKind::Cursor,
            HookOutcome::UpdatePreToolUse {
                system_message: String::from("Nudge substituted a command."),
                additional_context: String::from("rewrote npm to yarn"),
                updated_input: serde_json::json!({ "command": "yarn add foo" }),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(json["permission"], Value::String(String::from("allow")));
        pretty_assert_eq!(
            json["updated_input"]["command"],
            Value::String(String::from("yarn add foo"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["updatedInput"]["command"],
            Value::String(String::from("yarn add foo"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["additionalContext"],
            Value::String(String::from("rewrote npm to yarn"))
        );
    }

    #[test]
    fn cursor_user_prompt_context_continues_with_additional_context() {
        let rendered = render(
            AgentKind::Cursor,
            HookOutcome::AddContext {
                context: String::from("remember this"),
            },
        )
        .expect("render");

        let RenderedHookOutcome::Stdout(output) = rendered else {
            panic!("expected stdout");
        };
        let json = serde_json::from_str::<Value>(&output).expect("valid json");
        pretty_assert_eq!(json["continue"], Value::Bool(true));
        pretty_assert_eq!(
            json["hookSpecificOutput"]["hookEventName"],
            Value::String(String::from("UserPromptSubmit"))
        );
        pretty_assert_eq!(
            json["hookSpecificOutput"]["additionalContext"],
            Value::String(String::from("remember this"))
        );
    }

    #[test]
    fn cursor_passthrough_renders_no_output() {
        let rendered = render(AgentKind::Cursor, HookOutcome::Passthrough).expect("render");
        pretty_assert_eq!(rendered, RenderedHookOutcome::NoOutput);
    }
}
