//! Grok Build hook adapter.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Context, OptionExt, Result, eyre};
use serde_json::Value;

use crate::{
    agent::AgentKind,
    hook::{
        BashInput, DeleteInput, EditInput, HookContext, NudgeHook, PermissionRequest, PreToolUse,
        ToolUse, UserPromptSubmit, WebFetchInput, WriteInput,
    },
};

/// Parse a Grok Build hook payload into normalized Nudge hooks.
///
/// Native Grok envelopes use camelCase fields and snake_case event values
/// (`pre_tool_use`). Grok also forwards Claude-shaped snake_case payloads when
/// a Claude hook file is the registration source. Accept both.
pub fn parse_hook(raw: Value) -> Result<Vec<NudgeHook>> {
    let event = event_name(&raw)?;
    let context = context(&raw, AgentKind::Grok)?;

    match event.as_str() {
        "PreToolUse" => Ok(vec![NudgeHook::PreToolUse(PreToolUse {
            tool_input: field(&raw, "tool_input", "toolInput")
                .cloned()
                .unwrap_or(Value::Null),
            tool: tool_use(&raw, &context)?,
            context,
        })]),
        "PermissionRequest" => Ok(vec![NudgeHook::PermissionRequest(PermissionRequest {
            tool: tool_use(&raw, &context)?,
            context,
        })]),
        "UserPromptSubmit" => Ok(vec![NudgeHook::UserPromptSubmit(UserPromptSubmit {
            prompt: string_field(&raw, "prompt", "prompt")?.to_string(),
            context,
        })]),
        _ => Ok(vec![NudgeHook::Other]),
    }
}

fn event_name(raw: &Value) -> Result<String> {
    let event = string_field(raw, "hook_event_name", "hookEventName")?;
    Ok(match event {
        "pre_tool_use" | "PreToolUse" => String::from("PreToolUse"),
        "user_prompt_submit" | "UserPromptSubmit" => String::from("UserPromptSubmit"),
        "permission_request" | "PermissionRequest" => String::from("PermissionRequest"),
        other => other.to_string(),
    })
}

fn context(raw: &Value, agent: AgentKind) -> Result<HookContext> {
    let cwd = field(raw, "cwd", "cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(env::current_dir)
        .context("get hook cwd")?;

    Ok(HookContext {
        agent,
        session_id: optional_string(raw, "session_id", "sessionId"),
        turn_id: optional_string(raw, "turn_id", "promptId"),
        transcript_path: optional_string(raw, "transcript_path", "transcriptPath")
            .map(PathBuf::from),
        cwd,
        permission_mode: optional_string(raw, "permission_mode", "permissionMode"),
        model: optional_string(raw, "model", "model"),
    })
}

fn tool_use(raw: &Value, context: &HookContext) -> Result<ToolUse> {
    let tool_name = string_field(raw, "tool_name", "toolName")?;
    let input = field(raw, "tool_input", "toolInput")
        .cloned()
        .unwrap_or(Value::Null);

    match normalize_tool_name(tool_name) {
        NormalizedTool::Write => Ok(ToolUse::Write(WriteInput {
            file_path: path_field(&input)?,
            content: write_content(&input)?,
        })),
        NormalizedTool::Edit => {
            let file_path = path_field(&input)?;
            let old_string = optional_string(&input, "old_string", "oldString").unwrap_or_default();
            let new_string = string_field(&input, "new_string", "newString")?.to_string();

            if old_string.is_empty() {
                return Ok(ToolUse::Write(WriteInput {
                    file_path,
                    content: new_string,
                }));
            }

            let post_edit_content =
                post_edit_content(&context.cwd, &file_path, &old_string, &new_string);

            Ok(ToolUse::Edit(EditInput {
                file_path,
                old_string,
                new_string,
                post_edit_content,
            }))
        }
        NormalizedTool::Delete => Ok(ToolUse::Delete(DeleteInput {
            file_path: path_field(&input)?,
        })),
        NormalizedTool::WebFetch => Ok(ToolUse::WebFetch(WebFetchInput {
            url: string_field(&input, "url", "url")?.to_string(),
            prompt: optional_string(&input, "prompt", "prompt"),
        })),
        NormalizedTool::Bash => Ok(ToolUse::Bash(BashInput {
            command: string_field(&input, "command", "command")?.to_string(),
            description: optional_string(&input, "description", "description"),
        })),
        NormalizedTool::Other => Ok(ToolUse::Other {
            tool_name: tool_name.to_string(),
            input,
        }),
    }
}

enum NormalizedTool {
    Write,
    Edit,
    Delete,
    WebFetch,
    Bash,
    Other,
}

fn normalize_tool_name(tool_name: &str) -> NormalizedTool {
    match tool_name {
        "Write" | "write" | "write_file" | "create_file" => NormalizedTool::Write,
        "Edit" | "MultiEdit" | "search_replace" | "edit_file" => NormalizedTool::Edit,
        "Delete" | "delete_file" => NormalizedTool::Delete,
        "WebFetch" | "web_fetch" => NormalizedTool::WebFetch,
        "Bash" | "run_terminal_command" => NormalizedTool::Bash,
        _ => NormalizedTool::Other,
    }
}

fn post_edit_content(
    cwd: &Path,
    file_path: &Path,
    old_string: &str,
    new_string: &str,
) -> Option<String> {
    if old_string.is_empty() {
        return None;
    }

    let path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        cwd.join(file_path)
    };

    let current = fs::read_to_string(path).ok()?;
    current
        .contains(old_string)
        .then(|| current.replacen(old_string, new_string, 1))
}

fn field<'a>(value: &'a Value, snake: &str, camel: &str) -> Option<&'a Value> {
    match (value.get(snake), value.get(camel)) {
        (Some(snake_value), Some(camel_value)) if snake_value != camel_value => Some(snake_value),
        (Some(value), _) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn string_field<'a>(value: &'a Value, snake: &str, camel: &str) -> Result<&'a str> {
    field(value, snake, camel)
        .and_then(Value::as_str)
        .ok_or_else(|| eyre!("missing string field {snake}/{camel}"))
}

fn optional_string(value: &Value, snake: &str, camel: &str) -> Option<String> {
    field(value, snake, camel)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn path_field(value: &Value) -> Result<PathBuf> {
    optional_string(value, "file_path", "filePath")
        .or_else(|| optional_string(value, "path", "path"))
        .or_else(|| optional_string(value, "target_file", "targetFile"))
        .map(PathBuf::from)
        .ok_or_eyre("missing file path field file_path/path/target_file")
}

fn write_content(value: &Value) -> Result<String> {
    optional_string(value, "content", "content")
        .or_else(|| optional_string(value, "new_string", "newString"))
        .ok_or_eyre("missing write content field content/new_string")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use pretty_assertions::assert_eq as pretty_assert_eq;

    use crate::hook::{NudgeHook, ToolUse};

    use super::parse_hook;

    #[test]
    fn native_camel_case_bash_normalizes() {
        let hooks = parse_hook(json!({
            "hookEventName": "pre_tool_use",
            "sessionId": "abc",
            "cwd": "/tmp",
            "workspaceRoot": "/tmp",
            "permissionMode": "default",
            "toolName": "run_terminal_command",
            "toolInput": { "command": "cargo test", "description": "Run tests" }
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse");
        };
        let ToolUse::Bash(input) = &payload.tool else {
            panic!("expected Bash");
        };
        pretty_assert_eq!(input.command, "cargo test");
        pretty_assert_eq!(input.description.as_deref(), Some("Run tests"));
        pretty_assert_eq!(payload.context.session_id.as_deref(), Some("abc"));
        pretty_assert_eq!(payload.context.permission_mode.as_deref(), Some("default"));
    }

    #[test]
    fn snake_case_claude_compat_bash_normalizes() {
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
    fn dual_cased_payload_prefers_snake_case() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "hookEventName": "user_prompt_submit",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "toolName": "read_file",
            "tool_input": { "command": "cargo test" },
            "toolInput": { "target_file": "src.rs" }
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse from snake_case event");
        };
        assert!(
            matches!(payload.tool, ToolUse::Bash(_)),
            "snake_case tool_name should win over camelCase toolName"
        );
    }

    #[test]
    fn search_replace_with_old_string_normalizes_to_edit() {
        let hooks = parse_hook(json!({
            "hookEventName": "PreToolUse",
            "cwd": "/tmp",
            "toolName": "search_replace",
            "toolInput": {
                "file_path": "src.rs",
                "old_string": "fn old()",
                "new_string": "fn new()"
            }
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse");
        };
        let ToolUse::Edit(input) = &payload.tool else {
            panic!("expected Edit, got {:?}", payload.tool);
        };
        pretty_assert_eq!(input.file_path, PathBuf::from("src.rs"));
        pretty_assert_eq!(input.old_string, "fn old()");
        pretty_assert_eq!(input.new_string, "fn new()");
    }

    #[test]
    fn search_replace_without_old_string_normalizes_to_write() {
        let hooks = parse_hook(json!({
            "hookEventName": "pre_tool_use",
            "cwd": "/tmp",
            "toolName": "search_replace",
            "toolInput": {
                "file_path": "src.rs",
                "old_string": "",
                "new_string": "fn main() {}"
            }
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse");
        };
        let ToolUse::Write(input) = &payload.tool else {
            panic!(
                "expected Write for empty old_string, got {:?}",
                payload.tool
            );
        };
        pretty_assert_eq!(input.content, "fn main() {}");
    }

    #[test]
    fn write_file_alias_uses_path_and_content() {
        let hooks = parse_hook(json!({
            "hookEventName": "pre_tool_use",
            "cwd": "/tmp",
            "toolName": "write_file",
            "toolInput": { "path": "src.rs", "content": "fn main() {}" }
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse");
        };
        let ToolUse::Write(input) = &payload.tool else {
            panic!("expected Write");
        };
        pretty_assert_eq!(input.file_path, PathBuf::from("src.rs"));
        pretty_assert_eq!(input.content, "fn main() {}");
    }

    #[test]
    fn native_write_tool_normalizes_to_write() {
        // Grok Build 1.0.4 emits `write` with `file_path` for new files.
        let hooks = parse_hook(json!({
            "hookEventName": "pre_tool_use",
            "cwd": "/tmp",
            "toolName": "write",
            "toolInput": { "file_path": "/tmp/note.txt", "content": "hello\n" },
            "toolInputTruncated": false
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse");
        };
        let ToolUse::Write(input) = &payload.tool else {
            panic!("expected Write");
        };
        pretty_assert_eq!(input.file_path, PathBuf::from("/tmp/note.txt"));
        pretty_assert_eq!(input.content, "hello\n");
    }

    #[test]
    fn create_file_and_edit_file_aliases_normalize() {
        let write = parse_hook(json!({
            "hookEventName": "PreToolUse",
            "cwd": "/tmp",
            "toolName": "create_file",
            "toolInput": { "file_path": "new.rs", "content": "pub fn x() {}" }
        }))
        .expect("parse write");
        assert!(
            matches!(write.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Write(_)))
        );

        let edit = parse_hook(json!({
            "hookEventName": "PreToolUse",
            "cwd": "/tmp",
            "toolName": "edit_file",
            "toolInput": { "path": "src.rs", "old_string": "a", "new_string": "b" }
        }))
        .expect("parse edit");
        assert!(
            matches!(edit.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Edit(_)))
        );
    }

    #[test]
    fn web_fetch_alias_normalizes() {
        let hooks = parse_hook(json!({
            "hookEventName": "pre_tool_use",
            "cwd": "/tmp",
            "toolName": "web_fetch",
            "toolInput": { "url": "https://docs.rs/nudge" }
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse");
        };
        let ToolUse::WebFetch(input) = &payload.tool else {
            panic!("expected WebFetch");
        };
        pretty_assert_eq!(input.url, "https://docs.rs/nudge");
    }

    #[test]
    fn user_prompt_submit_accepts_snake_and_camel_event_names() {
        for event in ["UserPromptSubmit", "user_prompt_submit"] {
            let hooks = parse_hook(json!({
                "hookEventName": event,
                "cwd": "/tmp",
                "prompt": "hello"
            }))
            .expect("parse hook");

            assert!(
                matches!(hooks.as_slice(), [NudgeHook::UserPromptSubmit(payload)] if payload.prompt == "hello"),
                "event {event} should normalize to UserPromptSubmit"
            );
        }
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
    fn unknown_event_and_tool_pass_through() {
        let other_event = parse_hook(json!({
            "hookEventName": "session_start",
            "cwd": "/tmp"
        }))
        .expect("parse event");
        assert!(matches!(other_event.as_slice(), [NudgeHook::Other]));

        let other_tool = parse_hook(json!({
            "hookEventName": "pre_tool_use",
            "cwd": "/tmp",
            "toolName": "read_file",
            "toolInput": { "target_file": "src.rs" }
        }))
        .expect("parse tool");
        assert!(
            matches!(other_tool.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Other { .. }))
        );
    }

    #[test]
    fn missing_event_name_errors() {
        let error = parse_hook(json!({ "cwd": "/tmp" })).expect_err("missing event");
        assert!(error.to_string().contains("hook_event_name"));
    }
}
