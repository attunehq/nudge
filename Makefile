.PHONY: help format check check-fix test build precommit install

.DEFAULT_GOAL := help

help:
	@echo "Available commands:"
	@echo "  make format      - Format code with cargo +nightly fmt"
	@echo "  make check       - Run clippy linter"
	@echo "  make check-fix   - Run clippy with automatic fixes"
	@echo "  make test        - Run tests"
	@echo "  make build       - Build in debug mode"
	@echo "  make precommit   - Run all checks and fixes before committing"
	@echo "  make install     - Install nudge locally"

format:
	cargo +nightly fmt

check:
	cargo clippy --all-targets --all-features -- -D warnings

check-fix:
	cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged

test:
	cargo test --all-features

build:
	cargo build --all-targets

precommit: check-fix format
	@echo "Precommit checks complete!"

install:
	cargo install --path packages/nudge --locked --force
