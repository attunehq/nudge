//! Responds to Codex hooks.

use std::io;

use clap::Args;
use color_eyre::{Result, eyre::Context};
use nudge::{
    agent::{AgentKind, codex},
    hook::{
        evaluate::{evaluate_hooks, evaluate_hooks_with_state},
        response,
        state::{InteractionState, rules_need_interaction_state},
    },
    rules,
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
    let outcome = if rules_need_interaction_state(&rules) {
        let mut state = InteractionState::load().context("load interaction state")?;
        let outcome = evaluate_hooks_with_state(&hooks, &rules, &mut state);
        state.save().context("save interaction state")?;
        outcome
    } else {
        evaluate_hooks(&hooks, &rules)
    };

    response::emit(AgentKind::Codex, outcome)
}
