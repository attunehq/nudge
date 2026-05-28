//! Codex CLI hook adapter.

use std::{env, path::PathBuf};

use color_eyre::eyre::{Context, OptionExt, Result};
use serde_json::Value;

use crate::{
    agent::AgentKind,
    hook::{
        BashInput, HookContext, NudgeHook, PermissionRequest, PreToolUse, ToolUse,
        UserPromptSubmit, apply_patch,
    },
};

/// Parse a Codex hook payload into normalized Nudge hooks.
pub fn parse_hook(raw: Value) -> Result<Vec<NudgeHook>> {
    let event = raw
        .get("hook_event_name")
        .and_then(Value::as_str)
        .ok_or_eyre("missing hook_event_name")?;
    let context = context(&raw, AgentKind::Codex)?;

    match event {
        "PreToolUse" => Ok(pretooluse(raw, context)),
        "PermissionRequest" => Ok(vec![NudgeHook::PermissionRequest(PermissionRequest {
            tool: tool_use(&raw),
            context,
        })]),
        "UserPromptSubmit" => Ok(vec![NudgeHook::UserPromptSubmit(UserPromptSubmit {
            prompt: string_field(&raw, "prompt")?.to_string(),
            context,
        })]),
        _ => Ok(vec![NudgeHook::Other]),
    }
}

fn pretooluse(raw: Value, context: HookContext) -> Vec<NudgeHook> {
    let tool = tool_use(&raw);
    match tool {
        ToolUse::Other { tool_name, input } if tool_name == "apply_patch" => {
            let command = input
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match apply_patch::parse(command, &context.cwd) {
                Ok(tools) => tools
                    .into_iter()
                    .map(|tool| {
                        NudgeHook::PreToolUse(PreToolUse {
                            context: context.clone(),
                            tool,
                        })
                    })
                    .collect(),
                Err(error) => {
                    tracing::warn!(?error, "failed to parse Codex apply_patch input");
                    vec![NudgeHook::Other]
                }
            }
        }
        tool => vec![NudgeHook::PreToolUse(PreToolUse { context, tool })],
    }
}

fn context(raw: &Value, agent: AgentKind) -> Result<HookContext> {
    let cwd = raw
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(env::current_dir)
        .context("get hook cwd")?;

    Ok(HookContext {
        agent,
        session_id: optional_string(raw, "session_id"),
        turn_id: optional_string(raw, "turn_id"),
        transcript_path: optional_string(raw, "transcript_path").map(PathBuf::from),
        cwd,
        permission_mode: optional_string(raw, "permission_mode"),
        model: optional_string(raw, "model"),
    })
}

fn tool_use(raw: &Value) -> ToolUse {
    let tool_name = raw
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("Other");
    let input = raw.get("tool_input").cloned().unwrap_or(Value::Null);

    match tool_name {
        "Bash" => ToolUse::Bash(BashInput {
            command: input
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            description: optional_string(&input, "description"),
        }),
        other => ToolUse::Other {
            tool_name: other.to_string(),
            input,
        },
    }
}

fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| color_eyre::eyre::eyre!("missing string field {field}"))
}

fn optional_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::TempDir;

    use crate::hook::{NudgeHook, ToolUse};

    use super::parse_hook;

    #[test]
    fn bash_payload_normalizes_to_bash() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test" }
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Bash(_)))
        );
    }

    #[test]
    fn permission_request_normalizes() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PermissionRequest",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test" }
        }))
        .expect("parse hook");

        assert!(matches!(
            hooks.as_slice(),
            [NudgeHook::PermissionRequest(_)]
        ));
    }

    #[test]
    fn user_prompt_submit_normalizes_to_prompt_text() {
        let hooks = parse_hook(json!({
            "hook_event_name": "UserPromptSubmit",
            "cwd": "/tmp",
            "prompt": "hello"
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::UserPromptSubmit(payload)] if payload.prompt == "hello")
        );
    }

    #[test]
    fn apply_patch_add_file_normalizes_to_write() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "apply_patch",
            "tool_input": {
                "command": "*** Begin Patch\n*** Add File: test.rs\n+fn main() {}\n*** End Patch\n"
            }
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Write(_)))
        );
    }

    #[test]
    fn apply_patch_update_file_normalizes_to_edit() {
        let temp = TempDir::new().expect("temp dir");
        fs::write(temp.path().join("test.rs"), "fn main() {\n    old();\n}\n").expect("write file");

        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": temp.path(),
            "tool_name": "apply_patch",
            "tool_input": {
                "command": "*** Begin Patch\n*** Update File: test.rs\n@@\n fn main() {\n-    old();\n+    new();\n }\n*** End Patch\n"
            }
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Edit(_)))
        );
    }

    #[test]
    fn apply_patch_delete_file_normalizes_to_delete() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "apply_patch",
            "tool_input": {
                "command": "*** Begin Patch\n*** Delete File: test.rs\n*** End Patch\n"
            }
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Delete(_)))
        );
    }

    #[test]
    fn unsupported_mcp_tool_passes_through_as_other() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "mcp__server__tool",
            "tool_input": { "x": 1 }
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Other { .. }))
        );
    }
}
