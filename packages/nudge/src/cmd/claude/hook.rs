//! Responds to Claude Code hooks.

use std::io;

use clap::Args;
use color_eyre::{Result, eyre::Context};
use nudge::{
    agent::{AgentKind, claude},
    hook::{evaluate::evaluate_config_hooks, response},
    rules,
};
use tracing::instrument;

#[derive(Args, Clone, Debug)]
pub struct Config {}

#[instrument]
pub fn main(_config: Config) -> Result<()> {
    let stdin = io::stdin();
    let raw = serde_json::from_reader(stdin).context("read hook event")?;
    let hooks = claude::parse_hook(raw).context("parse Claude hook event")?;

    let config = rules::load_all().context("load rules")?;
    response::emit(AgentKind::Claude, evaluate_config_hooks(&hooks, &config)?)
}
