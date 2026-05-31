//! Opt-in workflow completion gates.

use std::{
    collections::{HashSet, hash_map::DefaultHasher},
    env, fs,
    hash::{Hash, Hasher},
    path::PathBuf,
};

use color_eyre::eyre::{Context, Result};
use indoc::formatdoc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    hook::{NudgeHook, Stop, UserPromptSubmit, response::HookOutcome},
    rules::{self, Workflow},
};

/// Evaluate workflow hooks and update per-session workflow state.
pub fn evaluate_hooks(hooks: &[NudgeHook], workflows: &[Workflow]) -> Result<HookOutcome> {
    if workflows.is_empty() {
        return Ok(HookOutcome::Passthrough);
    }

    let mut contexts = Vec::new();

    for hook in hooks {
        match hook {
            NudgeHook::UserPromptSubmit(payload) => {
                if let Some(context) = activate_workflows(payload, workflows)? {
                    contexts.push(context);
                }
            }
            NudgeHook::Stop(payload) => {
                return evaluate_stop(payload, workflows);
            }
            NudgeHook::PreToolUse(_) | NudgeHook::PermissionRequest(_) | NudgeHook::Other => {}
        }
    }

    if contexts.is_empty() {
        Ok(HookOutcome::Passthrough)
    } else {
        Ok(HookOutcome::AddContext {
            context: contexts.join("\n\n"),
        })
    }
}

fn activate_workflows(
    payload: &UserPromptSubmit,
    workflows: &[Workflow],
) -> Result<Option<String>> {
    let active = workflows
        .iter()
        .filter(|workflow| workflow.matches_prompt(&payload.prompt))
        .map(|workflow| ActiveWorkflow {
            name: workflow.name.clone(),
            prompt: payload.prompt.clone(),
            done: workflow.done.clone(),
            confirmation_token: workflow.confirmation_token(),
        })
        .collect::<Vec<_>>();

    if active.is_empty() {
        clear_state(&payload.context)?;
        return Ok(None);
    }

    write_state(
        &payload.context,
        &WorkflowState {
            version: 1,
            active: active.clone(),
        },
    )?;

    Ok(Some(activation_context(&active)))
}

fn evaluate_stop(payload: &Stop, workflows: &[Workflow]) -> Result<HookOutcome> {
    let Some(mut state) = read_state(&payload.context)? else {
        return Ok(HookOutcome::Passthrough);
    };

    let configured_workflows = workflows
        .iter()
        .map(|workflow| workflow.name.as_str())
        .collect::<HashSet<_>>();
    state
        .active
        .retain(|workflow| configured_workflows.contains(workflow.name.as_str()));

    if state.active.is_empty() {
        clear_state(&payload.context)?;
        return Ok(HookOutcome::Passthrough);
    }

    let last_assistant_message = payload
        .last_assistant_message
        .as_deref()
        .unwrap_or_default();
    let pending = state
        .active
        .iter()
        .filter(|workflow| !last_assistant_message.contains(&workflow.confirmation_token))
        .cloned()
        .collect::<Vec<_>>();

    if pending.is_empty() {
        clear_state(&payload.context)?;
        return Ok(HookOutcome::Passthrough);
    }

    Ok(HookOutcome::ContinueStop {
        reason: continuation_prompt(&pending, payload.stop_hook_active),
    })
}

fn activation_context(active: &[ActiveWorkflow]) -> String {
    active.iter().map(activation_context_for).join("\n\n")
}

fn activation_context_for(workflow: &ActiveWorkflow) -> String {
    let criteria = criteria_list(&workflow.done);
    formatdoc! {"
        Nudge workflow `{name}` is active for this prompt.

        Original user prompt:
        {prompt}

        Done criteria:
        {criteria}

        Before your final answer, verify every criterion against the actual work. If any criterion is incomplete, keep working. When every criterion is complete, include this exact line in your final assistant message:
        {token}
    ",
        name = workflow.name,
        prompt = workflow.prompt,
        criteria = criteria,
        token = workflow.confirmation_token,
    }
}

fn continuation_prompt(pending: &[ActiveWorkflow], stop_hook_active: bool) -> String {
    let retry_note = if stop_hook_active {
        "This is already a stop-hook continuation attempt. Do not repeat the same final answer unless you have verified the criteria."
    } else {
        "You are trying to stop before Nudge has seen workflow completion confirmation."
    };
    let workflows = pending.iter().map(continuation_prompt_for).join("\n\n");

    formatdoc! {"
        {retry_note}

        Evaluate the active workflow criteria below against the work you actually completed. If anything is missing, do the missing work and validation now. If every criterion is complete, respond with a concise final summary and include each exact confirmation line shown.

        {workflows}
    "}
}

fn continuation_prompt_for(workflow: &ActiveWorkflow) -> String {
    let criteria = criteria_list(&workflow.done);
    formatdoc! {"
        Workflow: `{name}`

        Original user prompt:
        {prompt}

        Done criteria:
        {criteria}

        Required confirmation line:
        {token}
    ",
        name = workflow.name,
        prompt = workflow.prompt,
        criteria = criteria,
        token = workflow.confirmation_token,
    }
}

fn criteria_list(done: &[String]) -> String {
    if done.is_empty() {
        return "- No explicit criteria configured. Confirm the user's prompt is fully satisfied."
            .to_string();
    }

    done.iter()
        .map(|criterion| format!("- {criterion}"))
        .join("\n")
}

fn read_state(context: &crate::hook::HookContext) -> Result<Option<WorkflowState>> {
    let path = state_path(context)?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read workflow state: {path:?}")),
    };

    serde_json::from_str(&content)
        .with_context(|| format!("parse workflow state: {path:?}"))
        .map(Some)
}

fn write_state(context: &crate::hook::HookContext, state: &WorkflowState) -> Result<()> {
    let path = state_path(context)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create workflow state directory: {parent:?}"))?;
    }
    let content = serde_json::to_string_pretty(state).context("serialize workflow state")?;
    fs::write(&path, content).with_context(|| format!("write workflow state: {path:?}"))
}

fn clear_state(context: &crate::hook::HookContext) -> Result<()> {
    let path = state_path(context)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove workflow state: {path:?}")),
    }
}

fn state_path(context: &crate::hook::HookContext) -> Result<PathBuf> {
    Ok(state_dir().join(format!("{}.json", state_key(context))))
}

fn state_dir() -> PathBuf {
    if let Some(path) = env::var_os("NUDGE_STATE_DIR") {
        return PathBuf::from(path);
    }

    rules::project_dirs()
        .map(|dirs| dirs.data_local_dir().join("workflow-state"))
        .unwrap_or_else(|| env::temp_dir().join("nudge-workflow-state"))
}

fn state_key(context: &crate::hook::HookContext) -> String {
    let mut hasher = DefaultHasher::new();
    format!("{:?}", context.agent).hash(&mut hasher);
    context.session_id.hash(&mut hasher);
    context.cwd.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowState {
    version: u8,
    active: Vec<ActiveWorkflow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveWorkflow {
    name: String,
    prompt: String,
    done: Vec<String>,
    confirmation_token: String,
}
