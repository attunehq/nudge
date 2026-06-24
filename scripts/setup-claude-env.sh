#!/usr/bin/env bash
set -euo pipefail

# setup-claude-env.sh
#
# Provision a fresh Claude Code cloud environment for nudge.
# Targets a Debian/Ubuntu Linux container that starts with nothing installed.
# Idempotent: safe to re-run. Invoke as: ./scripts/setup-claude-env.sh
#
# What it does:
#   - installs build tooling (git, build-essential, pkg-config)
#   - installs the Rust stable toolchain, plus nightly rustfmt (CI formats
#     with `cargo +nightly fmt`)
#   - fetches dependencies and warms the build cache
#
# nudge is a pure-Rust workspace (package in packages/nudge). No native
# system libraries, submodules, or secrets are needed to build or test.

GREEN='\033[0;32m'; YELLOW='\033[0;33m'; NC='\033[0m'
log()  { printf "${GREEN}==>${NC} %s\n" "$1"; }
warn() { printf "${YELLOW}warn:${NC} %s\n" "$1" >&2; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

SUDO=""
if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1; then SUDO="sudo"; fi

apt_install() {
  if ! command -v apt-get >/dev/null 2>&1; then
    warn "apt-get not found; please install manually: $*"
    return 0
  fi
  $SUDO apt-get update -y
  $SUDO DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$@"
}

install_rustup() {
  log "Installing Rust (stable)"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --profile minimal
}

load_cargo_env() {
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
  elif [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
}

log "Installing system build dependencies"
apt_install git build-essential pkg-config ca-certificates curl

if ! command -v rustup >/dev/null 2>&1; then
  install_rustup
fi
load_cargo_env

if ! command -v cargo >/dev/null 2>&1; then
  log "Installing Rust stable toolchain"
  rustup toolchain install stable --profile minimal
  rustup default stable
  load_cargo_env
fi

if ! command -v cargo >/dev/null 2>&1; then
  warn "cargo is still unavailable after Rust setup"
  exit 1
fi

log "Ensuring nightly rustfmt is available (used by 'cargo +nightly fmt')"
rustup toolchain install nightly --profile minimal --component rustfmt >/dev/null 2>&1 \
  || warn "could not install nightly rustfmt; formatting checks may be unavailable"

log "Adding clippy (used by CI lint gate)"
rustup component add clippy >/dev/null 2>&1 || warn "could not add clippy"

log "Fetching dependencies"
cargo fetch --locked || cargo fetch

log "Warming the build (cargo build --all-targets)"
cargo build --all-targets

log "nudge environment ready"
