//! Documentation for writing Nudge rules with Codex.

use clap::Args;
use color_eyre::Result;

use crate::cmd::claude::docs;

#[derive(Args, Clone, Debug)]
pub struct Config {}

pub fn main(_config: Config) -> Result<()> {
    docs::main(docs::Config {})
}
