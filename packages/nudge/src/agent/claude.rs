//! Claude Code hook adapter.

use std::{env, path::PathBuf};

use color_eyre::eyre::{Context, OptionExt, Result};
use serde_json::Value;

use crate::{
    agent::AgentKind,
    hook::{
        BashInput, DeleteInput, EditInput, HookContext, NudgeHook, PermissionRequest, PreToolUse,
        ToolUse, UserPromptSubmit, WebFetchInput, WriteInput,
    },
};

/// Parse a Claude Code hook payload into normalized Nudge hooks.
pub fn parse_hook(raw: Value) -> Result<Vec<NudgeHook>> {
    let event = raw
        .get("hook_event_name")
        .and_then(Value::as_str)
        .ok_or_eyre("missing hook_event_name")?;
    let context = context(&raw, AgentKind::Claude)?;

    match event {
        "PreToolUse" => Ok(vec![NudgeHook::PreToolUse(PreToolUse {
            tool_input: raw.get("tool_input").cloned().unwrap_or(Value::Null),
            tool: tool_use(&raw)?,
            context,
        })]),
        "PermissionRequest" => Ok(vec![NudgeHook::PermissionRequest(PermissionRequest {
            tool: tool_use(&raw)?,
            context,
        })]),
        "UserPromptSubmit" => Ok(vec![NudgeHook::UserPromptSubmit(UserPromptSubmit {
            prompt: string_field(&raw, "prompt")?.to_string(),
            context,
        })]),
        _ => Ok(vec![NudgeHook::Other]),
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

fn tool_use(raw: &Value) -> Result<ToolUse> {
    let tool_name = string_field(raw, "tool_name")?;
    let input = raw.get("tool_input").cloned().unwrap_or(Value::Null);

    match tool_name {
        "Write" => Ok(ToolUse::Write(WriteInput {
            file_path: path_field(&input, "file_path")?,
            content: string_field(&input, "content")?.to_string(),
        })),
        "Edit" => Ok(ToolUse::Edit(EditInput {
            file_path: path_field(&input, "file_path")?,
            old_string: string_field(&input, "old_string")
                .unwrap_or_default()
                .to_string(),
            new_string: string_field(&input, "new_string")?.to_string(),
        })),
        "Delete" => Ok(ToolUse::Delete(DeleteInput {
            file_path: path_field(&input, "file_path")?,
        })),
        "WebFetch" => Ok(ToolUse::WebFetch(WebFetchInput {
            url: string_field(&input, "url")?.to_string(),
            prompt: optional_string(&input, "prompt"),
        })),
        "Bash" => Ok(ToolUse::Bash(BashInput {
            command: string_field(&input, "command")?.to_string(),
            description: optional_string(&input, "description"),
        })),
        other => Ok(ToolUse::Other {
            tool_name: other.to_string(),
            input,
        }),
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

fn path_field(value: &Value, field: &str) -> Result<PathBuf> {
    string_field(value, field).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use pretty_assertions::assert_eq as pretty_assert_eq;

    use crate::hook::{NudgeHook, ToolUse};

    use super::parse_hook;

    #[test]
    fn write_payload_normalizes_to_write() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "session_id": "test",
            "transcript_path": "/tmp/test",
            "permission_mode": "default",
            "cwd": "/tmp",
            "tool_name": "Write",
            "tool_input": { "file_path": "src.rs", "content": "fn main() {}" }
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Write(_)))
        );
    }

    #[test]
    fn edit_payload_normalizes_to_edit() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "Edit",
            "tool_input": { "file_path": "src.rs", "old_string": "a", "new_string": "b" }
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Edit(_)))
        );
    }

    #[test]
    fn bash_payload_normalizes_to_bash() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test", "timeout": 120 }
        }))
        .expect("parse hook");

        assert!(
            matches!(hooks.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Bash(_)))
        );
        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse");
        };
        pretty_assert_eq!(payload.tool_input["timeout"], 120);
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
    fn mcp_tool_normalizes_to_other() {
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
