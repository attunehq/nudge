//! Responds to Codex hooks.

use std::io;

use clap::Args;
use color_eyre::{Result, eyre::Context};
use nudge::{
    agent::{AgentKind, codex},
    hook::{NudgeHook, evaluate::evaluate_hooks_with_learnings, response},
    learn, rules,
};
use tracing::instrument;

#[derive(Args, Clone, Debug)]
pub struct Config {}

#[instrument]
pub fn main(_config: Config) -> Result<()> {
    let stdin = io::stdin();
    let raw = serde_json::from_reader(stdin).context("read hook event")?;
    let hooks = codex::parse_hook(raw).context("parse Codex hook event")?;

    let rules = rules::load_all().context("load rules")?;
    let root = hook_root(&hooks);
    let learned_notes = learn::load_all(root).context("load learned notes")?;
    let learn_config = learn::load_config().context("load learn config")?;
    response::emit(
        AgentKind::Codex,
        evaluate_hooks_with_learnings(root, &hooks, &rules, &learned_notes, &learn_config),
    )
}

fn hook_root(hooks: &[NudgeHook]) -> &std::path::Path {
    hooks
        .iter()
        .find_map(|hook| match hook {
            NudgeHook::PreToolUse(payload) => Some(payload.context.cwd.as_path()),
            NudgeHook::PermissionRequest(payload) => Some(payload.context.cwd.as_path()),
            NudgeHook::UserPromptSubmit(payload) => Some(payload.context.cwd.as_path()),
            _ => None,
        })
        .unwrap_or_else(|| std::path::Path::new("."))
}
