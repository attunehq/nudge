# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Pavlov?

Pavlov hooks into Claude Code to enforce coding rules and inject guidance. It intercepts tool calls (Write, Edit) via Claude Code's hooks system and evaluates the content against a set of rules before allowing the operation to proceed.

## Build and Test Commands

```bash
# Build
cargo build -p pavlov

# Run all tests
cargo test -p pavlov

# Run a specific test
cargo test -p pavlov test_name

# Run the CLI
cargo run -p pavlov -- claude hook      # Respond to hook (reads JSON from stdin)
cargo run -p pavlov -- claude setup     # Install hooks into .claude/settings.json
```

## Architecture

### CLI Structure

```
pavlov claude hook   - Receives hook JSON on stdin, evaluates rules, outputs response
pavlov claude setup  - Writes hook configuration to .claude/settings.json
```

### Module Layout

- `src/main.rs` - CLI entry point using clap
- `src/cmd/claude/hook.rs` - Hook command: deserializes input, calls `rules::evaluate_all()`, emits response
- `src/cmd/claude/setup.rs` - Setup command: configures hooks in settings.json
- `src/rules.rs` - All rule functions and `evaluate_all()` dispatcher
- `src/claude/hook.rs` - Types for hook payloads and responses (Hook, Response, etc.)

### Hook Response Types

Rules return one of three responses:

- **Passthrough**: No opinion, tool proceeds silently
- **Continue**: Tool proceeds, but guidance message is injected into conversation (soft suggestion)
- **Interrupt**: Tool is blocked, message explains why (hard rule violation)

### Adding a New Rule

1. Add the rule function to `src/rules.rs`:
   ```rust
   fn my_rule(hook: &Hook) -> Response {
       // Extract file_path and content using extract_file_content()
       // Check conditions
       // Return Passthrough, Continue, or Interrupt
   }
   ```

2. Register it in `evaluate_all()`:
   ```rust
   let rules: &[fn(&Hook) -> Response] = &[
       // ... existing rules
       my_rule,
   ];
   ```

3. Add tests in `tests/rules.rs` using `simple_test_case` for parameterized tests

### Testing Pattern

Integration tests run the actual CLI via xshell:

```rust
#[test_case("content", Expected::Interrupt; "description")]
#[test]
fn test_my_rule(content: &str, expected: Expected) {
    let sh = Shell::new().unwrap();
    let input = write_hook("test.rs", content);  // Build hook JSON
    let (exit_code, stdout) = run_hook(&sh, &input);  // Run CLI
    // Assert based on expected
}
```

Use `pretty_assertions::assert_eq as pretty_assert_eq` to avoid conflicts with std prelude.
