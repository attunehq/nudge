# Undocumented Claude Code Hook Protocol Details

> **Note:** These observations are effective as of Claude Code v2.0.53 (November 2025). Behavior may change in future versions.

This document captures undocumented behaviors discovered through experimentation with Claude Code hooks. These details are not in the official documentation but are critical for hooks to work correctly.

## Exit Codes and Output Streams

### Exit Code 2 (Interrupt) Reads from stderr, NOT stdout

**This is the most critical undocumented behavior.**

When a hook exits with code 2 (interrupt/block), Claude Code reads the response from **stderr**, not stdout. Writing to stdout with exit code 2 results in the response being silently ignored.

```rust
// WRONG - response is ignored
Response::Interrupt(r) => {
    let json = serde_json::to_string(&r)?;
    println!("{json}");  // stdout - IGNORED for exit code 2
    std::process::exit(2);
}

// CORRECT - response is processed
Response::Interrupt(r) => {
    let json = serde_json::to_string(&r)?;
    eprintln!("{json}");  // stderr - read by Claude Code
    std::process::exit(2);
}
```

### Exit Code 0 (Continue/Passthrough) Reads from stdout

For exit code 0, Claude Code reads from stdout as expected:

```rust
Response::Continue(r) => {
    let json = serde_json::to_string(&r)?;
    println!("{json}");  // stdout - correct for exit code 0
    Ok(())
}
```

## Response Type Behavior

### Continue Responses Cannot Correct Past Behavior

Continue responses (`"continue": true`) allow the tool operation to proceed, then inject a message. However, this message arrives **after** Claude has already committed to the action.

This means Continue is ineffective for corrections because:
1. The file is already written/edited
2. Claude has mentally moved on to the next task
3. The guidance arrives too late to influence the decision

**Recommendation**: Use Interrupt for any rule you actually want enforced. Reserve Continue only for forward-looking guidance or logging.

### Interrupt Responses Require Explicit Retry Instructions

When Claude receives an interrupt, it sees the operation was blocked but may not automatically retry. To get Claude to fix and retry:

1. Use directive language in `stop_reason`: "BLOCKED: ... Fix and retry immediately."
2. Be specific in `system_message`: "Add a blank line after lines 10, 12, 14, then retry."
3. Keep messages short - long explanations get skimmed

**Effective pattern**:
```json
{
  "continue": false,
  "stopReason": "BLOCKED: Missing blank lines between struct fields. Fix and retry immediately.",
  "systemMessage": "Add a blank line after lines 10, 12, 14, then retry.",
  "suppressOutput": false,
  "hookSpecificOutput": {"hookEventName": "PreToolUse"}
}
```

**Ineffective pattern** (too verbose, not directive):
```json
{
  "continue": false,
  "stopReason": "Style guide violation",
  "systemMessage": "Found consecutive struct fields without blank lines (after lines: 10, 12, 14)\n\nPer the style guide, each field should be separated by a blank line.\n\nWhy this matters:\n  - Blank lines create visual separation...\n  [15 more lines of explanation]",
  ...
}
```

## Message Framing

### System Messages Should Sound Like System Commands

Claude responds better to messages that sound authoritative and mandatory rather than friendly suggestions.

**Effective**:
- "BLOCKED: Import statements inside function body. Fix and retry immediately."
- "Add a blank line after lines 10, 12, 14, then retry."

**Ineffective**:
- "Found indented 'use' statements in new content"
- "Per the project style guide, this pattern is discouraged..."
- "You might want to consider..."

### Include Specific Line Numbers

Always include specific line numbers when possible. This gives Claude actionable information:

```
"Remove LHS type annotations on lines 5, 12. Use turbofish instead, then retry."
```

## Hook Timing

### PreToolUse is Too Late for Reasoning Changes

Hooks run **after** Claude has decided what to do but **before** the tool executes. This means:

- You can block operations (Interrupt)
- You can inject messages (Continue)
- You **cannot** change Claude's reasoning about what to write

For rules that should influence initial code generation, put them in `CLAUDE.md`. Use hooks as a safety net to catch violations, not as the primary guidance mechanism.

## Testing Considerations

### Cargo Output Goes to stderr

When running hooks via `cargo run`, the cargo build/run messages go to stderr. In tests that combine stdout and stderr, use `--quiet` to suppress cargo output:

```rust
cmd!(sh, "cargo run --quiet -p pavlov -- claude hook")
```

## Summary Table

| Exit Code | Output Stream | Use Case |
|-----------|---------------|----------|
| 0 (no output) | N/A | Passthrough - no opinion |
| 0 (with JSON) | stdout | Continue - proceed with guidance |
| 2 | stderr | Interrupt - block and require fix |

