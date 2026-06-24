#!/usr/bin/env bash
# Claude Code cloud environment setup for nudge.
#
# Runs as root on Ubuntu 24.04 before the session starts, per
# https://code.claude.com/docs/en/claude-code-on-the-web#setup-scripts
# Point an environment's Setup script at:  bash scripts/setup-claude-env.sh
#
# Design rules (from the docs):
#   - Never block session start: every step is non-fatal and the script exits 0.
#   - Keep total runtime under ~5 minutes so the environment cache can build.
#   - Rust (rustc/cargo) and git are pre-installed; nudge is a pure-Rust
#     workspace (package in packages/nudge) with no native deps or secrets.
# Idempotent and cached; safe to re-run.

set -uo pipefail

log()  { printf '==> %s\n' "$1"; }
warn() { printf 'warn: %s\n' "$1" >&2; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

persist_path() {
  [ -n "${CLAUDE_ENV_FILE:-}" ] || return 0
  printf 'export PATH="%s:$PATH"\n' "$1" >> "$CLAUDE_ENV_FILE"
}

if ! command -v cargo >/dev/null 2>&1; then
  log "cargo not found; installing Rust via rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --profile minimal || warn "rustup install failed"
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env" 2>/dev/null || true
  persist_path "$HOME/.cargo/bin"
fi

# CI formats with nightly and lints with clippy; both optional for a build.
if command -v rustup >/dev/null 2>&1; then
  rustup toolchain install nightly --profile minimal --component rustfmt >/dev/null 2>&1 \
    || warn "nightly rustfmt unavailable; 'cargo +nightly fmt' may not work"
  rustup component add clippy >/dev/null 2>&1 || warn "could not add clippy"
fi

log "Fetching crates"
cargo fetch --locked || cargo fetch || warn "cargo fetch failed (check the environment's network access level)"

log "Warming the build (best-effort)"
cargo build --all-targets \
  || warn "cargo build did not finish; crates are fetched and the session can build in-session"

log "nudge environment ready"
exit 0
