//! Cursor / cursor-agent hook adapter.

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

/// Parse a Cursor hook payload into normalized Nudge hooks.
///
/// Native Cursor envelopes use snake_case fields and camelCase event values
/// (`preToolUse`, `beforeShellExecution`, `beforeSubmitPrompt`). Cursor also
/// accepts Claude-shaped payloads when a Claude hook file is the registration
/// source. Accept both.
pub fn parse_hook(raw: Value) -> Result<Vec<NudgeHook>> {
    let event = event_name(&raw)?;
    let context = context(&raw, AgentKind::Cursor)?;

    match event.as_str() {
        "PreToolUse" => Ok(vec![NudgeHook::PreToolUse(PreToolUse {
            tool_input: tool_input(&raw),
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
        "preToolUse" | "PreToolUse" | "beforeShellExecution" => String::from("PreToolUse"),
        "beforeSubmitPrompt" | "UserPromptSubmit" | "user_prompt_submit" => {
            String::from("UserPromptSubmit")
        }
        "PermissionRequest" | "permission_request" => String::from("PermissionRequest"),
        other => other.to_string(),
    })
}

fn context(raw: &Value, agent: AgentKind) -> Result<HookContext> {
    let cwd = field(raw, "cwd", "cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| first_workspace_root(raw))
        .map(Ok)
        .unwrap_or_else(env::current_dir)
        .context("get hook cwd")?;

    Ok(HookContext {
        agent,
        session_id: optional_string(raw, "conversation_id", "conversationId")
            .or_else(|| optional_string(raw, "session_id", "sessionId")),
        turn_id: optional_string(raw, "generation_id", "generationId")
            .or_else(|| optional_string(raw, "turn_id", "turnId")),
        transcript_path: optional_string(raw, "transcript_path", "transcriptPath")
            .map(PathBuf::from),
        cwd,
        permission_mode: optional_string(raw, "permission_mode", "permissionMode"),
        model: optional_string(raw, "model", "model"),
    })
}

fn first_workspace_root(raw: &Value) -> Option<PathBuf> {
    field(raw, "workspace_roots", "workspaceRoots")
        .and_then(Value::as_array)
        .and_then(|roots| roots.first())
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

fn tool_input(raw: &Value) -> Value {
    if let Some(input) = field(raw, "tool_input", "toolInput").cloned() {
        return input;
    }

    if let Some(command) = optional_string(raw, "command", "command") {
        return serde_json::json!({ "command": command });
    }

    Value::Null
}

fn tool_use(raw: &Value, context: &HookContext) -> Result<ToolUse> {
    let event = string_field(raw, "hook_event_name", "hookEventName").unwrap_or("");
    if event == "beforeShellExecution" {
        return Ok(ToolUse::Bash(BashInput {
            command: string_field(raw, "command", "command")?.to_string(),
            description: optional_string(raw, "description", "description"),
        }));
    }

    let tool_name = string_field(raw, "tool_name", "toolName")?;
    let input = field(raw, "tool_input", "toolInput")
        .cloned()
        .unwrap_or(Value::Null);

    match normalize_tool_name(tool_name, &input) {
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

fn normalize_tool_name(tool_name: &str, input: &Value) -> NormalizedTool {
    match tool_name {
        "Write" | "write_file" | "create_file" => {
            if optional_string(input, "old_string", "oldString").is_some()
                && optional_string(input, "new_string", "newString").is_some()
            {
                NormalizedTool::Edit
            } else {
                NormalizedTool::Write
            }
        }
        "Edit" | "StrReplace" | "search_replace" | "edit_file" => NormalizedTool::Edit,
        "Delete" | "delete_file" => NormalizedTool::Delete,
        "WebFetch" | "web_fetch" => NormalizedTool::WebFetch,
        "Shell" | "Bash" | "run_terminal_command" => NormalizedTool::Bash,
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
    optional_string(value, "contents", "contents")
        .or_else(|| optional_string(value, "content", "content"))
        .or_else(|| optional_string(value, "new_string", "newString"))
        .ok_or_eyre("missing write content field contents/content/new_string")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq as pretty_assert_eq;
    use serde_json::json;

    use crate::hook::{NudgeHook, ToolUse};

    use super::parse_hook;

    #[test]
    fn native_pretooluse_shell_normalizes() {
        let hooks = parse_hook(json!({
            "hook_event_name": "preToolUse",
            "conversation_id": "conv-1",
            "generation_id": "gen-1",
            "cwd": "/tmp",
            "model": "composer",
            "tool_name": "Shell",
            "tool_input": { "command": "cargo test", "working_directory": "/tmp" }
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse");
        };
        let ToolUse::Bash(input) = &payload.tool else {
            panic!("expected Bash");
        };
        pretty_assert_eq!(input.command, "cargo test");
        pretty_assert_eq!(payload.context.session_id.as_deref(), Some("conv-1"));
        pretty_assert_eq!(payload.context.turn_id.as_deref(), Some("gen-1"));
        pretty_assert_eq!(payload.context.model.as_deref(), Some("composer"));
        pretty_assert_eq!(payload.tool_input["working_directory"], "/tmp");
    }

    #[test]
    fn before_shell_execution_uses_top_level_command() {
        let hooks = parse_hook(json!({
            "hook_event_name": "beforeShellExecution",
            "conversation_id": "conv-1",
            "cwd": "/tmp",
            "command": "npm install lodash",
            "sandbox": false
        }))
        .expect("parse hook");

        let [NudgeHook::PreToolUse(payload)] = hooks.as_slice() else {
            panic!("expected PreToolUse from beforeShellExecution");
        };
        let ToolUse::Bash(input) = &payload.tool else {
            panic!("expected Bash");
        };
        pretty_assert_eq!(input.command, "npm install lodash");
        pretty_assert_eq!(payload.tool_input["command"], "npm install lodash");
    }

    #[test]
    fn write_uses_path_and_contents() {
        let hooks = parse_hook(json!({
            "hook_event_name": "preToolUse",
            "cwd": "/tmp",
            "tool_name": "Write",
            "tool_input": { "path": "src.rs", "contents": "fn main() {}" }
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
    fn write_with_old_and_new_string_normalizes_to_edit() {
        let hooks = parse_hook(json!({
            "hook_event_name": "preToolUse",
            "cwd": "/tmp",
            "tool_name": "Write",
            "tool_input": {
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
    fn write_with_empty_old_string_normalizes_to_write() {
        let hooks = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "Write",
            "tool_input": {
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
    fn delete_and_webfetch_normalize() {
        let delete = parse_hook(json!({
            "hook_event_name": "preToolUse",
            "cwd": "/tmp",
            "tool_name": "Delete",
            "tool_input": { "path": "src.rs" }
        }))
        .expect("parse delete");
        assert!(
            matches!(delete.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Delete(_)))
        );

        let fetch = parse_hook(json!({
            "hook_event_name": "preToolUse",
            "cwd": "/tmp",
            "tool_name": "WebFetch",
            "tool_input": { "url": "https://docs.rs/nudge" }
        }))
        .expect("parse fetch");
        let [NudgeHook::PreToolUse(payload)] = fetch.as_slice() else {
            panic!("expected PreToolUse");
        };
        let ToolUse::WebFetch(input) = &payload.tool else {
            panic!("expected WebFetch");
        };
        pretty_assert_eq!(input.url, "https://docs.rs/nudge");
    }

    #[test]
    fn before_submit_prompt_normalizes() {
        let hooks = parse_hook(json!({
            "hook_event_name": "beforeSubmitPrompt",
            "conversation_id": "conv-1",
            "workspace_roots": ["/tmp/project"],
            "prompt": "hello"
        }))
        .expect("parse hook");

        let [NudgeHook::UserPromptSubmit(payload)] = hooks.as_slice() else {
            panic!("expected UserPromptSubmit");
        };
        pretty_assert_eq!(payload.prompt, "hello");
        pretty_assert_eq!(payload.context.cwd, PathBuf::from("/tmp/project"));
        pretty_assert_eq!(payload.context.session_id.as_deref(), Some("conv-1"));
    }

    #[test]
    fn claude_compat_event_names_still_normalize() {
        let prompt = parse_hook(json!({
            "hook_event_name": "UserPromptSubmit",
            "cwd": "/tmp",
            "prompt": "hello"
        }))
        .expect("parse prompt");
        assert!(
            matches!(prompt.as_slice(), [NudgeHook::UserPromptSubmit(payload)] if payload.prompt == "hello")
        );

        let bash = parse_hook(json!({
            "hook_event_name": "PreToolUse",
            "cwd": "/tmp",
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test" }
        }))
        .expect("parse bash");
        assert!(
            matches!(bash.as_slice(), [NudgeHook::PreToolUse(payload)] if matches!(payload.tool, ToolUse::Bash(_)))
        );
    }

    #[test]
    fn dual_cased_payload_prefers_snake_case() {
        let hooks = parse_hook(json!({
            "hook_event_name": "preToolUse",
            "hookEventName": "beforeSubmitPrompt",
            "cwd": "/tmp",
            "tool_name": "Shell",
            "toolName": "Read",
            "tool_input": { "command": "cargo test" },
            "toolInput": { "path": "src.rs" }
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
    fn unknown_event_and_tool_pass_through() {
        let other_event = parse_hook(json!({
            "hook_event_name": "sessionStart",
            "cwd": "/tmp"
        }))
        .expect("parse event");
        assert!(matches!(other_event.as_slice(), [NudgeHook::Other]));

        let other_tool = parse_hook(json!({
            "hook_event_name": "preToolUse",
            "cwd": "/tmp",
            "tool_name": "Read",
            "tool_input": { "path": "src.rs" }
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
