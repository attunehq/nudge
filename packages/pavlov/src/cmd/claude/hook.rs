//! Responds to Claude Code hooks.

use clap::Args;
use color_eyre::{Result, eyre::Context};
use pavlov::claude::hook::{Hook, Response};
use pavlov::rules;
use tracing::instrument;

#[derive(Args, Clone, Debug)]
pub struct Config {}

#[instrument]
pub fn main(_config: Config) -> Result<()> {
    let stdin = std::io::stdin();
    let hook = serde_json::from_reader::<_, Hook>(stdin).context("read hook event")?;
    tracing::debug!(?hook, "claude_code.hook.read");

    let response = rules::evaluate_all(&hook);
    tracing::debug!(?response, "claude_code.hook.response");

    emit_response(response)
}

fn emit_response(response: Response) -> Result<()> {
    match response {
        Response::Passthrough => Ok(()),
        Response::Continue(r) => {
            let json = serde_json::to_string(&r).context("serialize continue response")?;
            println!("{json}");
            Ok(())
        }
        Response::Interrupt(r) => {
            let json = serde_json::to_string(&r).context("serialize interrupt response")?;
            eprintln!("{json}");
            std::process::exit(2);
        }
        _ => Ok(()),
    }
}
