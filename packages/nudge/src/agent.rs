//! Agent-specific hook adapters.

use serde::{Deserialize, Serialize};

pub mod claude;
pub mod codex;

/// The agent that emitted a hook event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum AgentKind {
    /// Claude Code.
    Claude,

    /// Codex CLI.
    Codex,
}
