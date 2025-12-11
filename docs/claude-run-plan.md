# Implementation Plan: `nudge claude run`

## Overview

A subcommand that wraps the Claude Code CLI, allowing Nudge to control the interaction between the user and Claude. This enables features that aren't possible with hooks alone:

- Running the agent in a loop (agentic mode)
- Session resuming across instances
- Session teleporting between users and machines
- Rewindable sessions that couple conversation state with code state
- Providing a working terminal frontend for users whose terminals don't work with Claude's built-in UI

## Status

**Phase 1 (MVP) is complete.** The basic subprocess wrapper works:

```bash
nudge claude run                           # Interactive prompt
nudge claude run "your prompt here"        # With initial prompt
nudge claude run -c                        # Continue most recent session
nudge claude run -r <session_id>           # Resume specific session
nudge claude run -v                        # Verbose (show tool I/O)
nudge claude run --max-turns 50            # Limit agentic turns
nudge claude run --model sonnet            # Specify model
```

## How Claude Code's JSON API Works

### Command-Line Flags

```bash
claude -p \
  --output-format stream-json \
  --input-format stream-json \
  --verbose
```

- `-p` / `--print`: Non-interactive mode (required for JSON I/O)
- `--output-format stream-json`: Emit NDJSON on stdout
- `--input-format stream-json`: Accept NDJSON messages on stdin (prompt sent via stdin, not as argument)
- `--verbose`: Required with stream-json output
- `--continue` / `-c`: Resume most recent conversation
- `--resume <session_id>`: Resume specific session by ID

### Output Message Types (NDJSON on stdout)

```jsonl
{"type":"system","subtype":"init","session_id":"...","cwd":"...","model":"...","tools":[...]}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"..."}],"stop_reason":null}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"...","content":"..."}]}}
{"type":"result","subtype":"success","session_id":"...","duration_ms":1234,"total_cost_usd":0.05}
```

### Input Message Format (NDJSON on stdin)

```json
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"your message"}]}}
```

## Architecture

### Module Structure

```
src/cmd/claude/
├── claude.rs       # Commands enum includes Run
├── run.rs          # Main run command, CLI config, run loop
├── run/
│   ├── process.rs  # ClaudeProcess subprocess management
│   ├── stream.rs   # NDJSON message types
│   └── ui.rs       # Terminal UI
├── hook.rs
├── setup.rs
└── docs.rs
```

### Key Types

**`ClaudeProcess`** (`run/process.rs`): Spawns and manages the Claude subprocess.
- `spawn(opts)` - Start Claude with stream-json flags, send initial prompt via stdin
- `send_message(msg)` - Write JSON to stdin
- `read_message()` - Read next NDJSON line from stdout

**`OutputMessage`** (`run/stream.rs`): Enum for parsing Claude's output:
- `System` - Init message with session_id, tools, model
- `Assistant` - Text and tool_use content blocks
- `User` - Tool results
- `Result` - Turn completion with stats

**`TerminalUI`** (`run/ui.rs`): Handles display and user input:
- Displays assistant text, tool calls with summaries, results
- Tool summaries show contextual info (file paths, commands, patterns)
- Prompts user for input with slash command support

## CLI Interface

```bash
nudge claude run [OPTIONS] [PROMPT]

Arguments:
  [PROMPT]  Initial prompt to send to Claude

Options:
  -c, --continue               Continue the most recent conversation
  -r, --resume <RESUME>        Resume a specific session by ID
      --max-turns <MAX_TURNS>  Maximum number of agentic turns
      --model <MODEL>          Model to use
  -v, --verbose                Show verbose output (tool inputs/outputs)
      --cwd <CWD>              Working directory
```

### Slash Commands (in-session)

- `/exit`, `/quit`, `/q` - Exit the conversation
- `/help`, `/h`, `/?` - Show help
- `/session` - Show current session ID

## Future Phases

### Phase 2: Rewindable Sessions

**Goal**: Bundle conversation context with code state so you can rewind to any point in time.

A session captures:
1. **Starting point**: repo URL + commit hash (or local path + initial state)
2. **Conversation log**: Claude's NDJSON stream (messages, tool calls, results)
3. **Change log**: Diffs keyed to conversation turns

This enables:
- **Rewind**: Jump back to turn N, which also reverts code to that point
- **Teleport**: Share a session bundle with another user who can replay/continue
- **Fork**: Branch from any point to explore different approaches
- **Non-destructive compaction**: `/compact` forks with a summary instead of nuking history

#### Session Bundle Format

```
session-<id>/
├── manifest.json      # Metadata: repo, commit, created_at, turn_count
├── conversation.jsonl # Claude's stream (what we already capture)
└── changes/
    ├── turn-001.patch # Diff after turn 1
    ├── turn-002.patch # Diff after turn 2 (cumulative or incremental TBD)
    └── ...
```

#### Key Operations

- `nudge session list` - List sessions with metadata
- `nudge session export <id>` - Bundle session for sharing
- `nudge session import <bundle>` - Import and set up session
- `nudge session rewind <id> <turn>` - Rewind to specific turn
- `nudge session fork <id> [turn]` - Fork from a point
- `/compact` (in-session) - Fork current session with a summarized context (original preserved)

#### Open Questions

- Cumulative vs incremental patches? (cumulative = simpler to apply, incremental = smaller)
- How to handle uncommitted changes at session start?
- What about files outside the repo (e.g., config, env)?

### Phase 3: Agentic Loop Control

- Local turn counting independent of Claude's `--max-turns`
- Loop detection and intervention
- Configurable intervention points

### Phase 4: Rule Integration

When we see `tool_use` messages for Write/Edit:
1. Parse the tool input using existing hook types
2. Run Nudge rules against it
3. If rules match, inject feedback via stdin message
4. More control than hooks (can modify behavior, not just block)

### Phase 5: Enhanced UI

- Consider `ratatui` for TUI with scrolling
- Syntax highlighting for code
- Progress indicators during tool execution

## Design Decisions

1. **Stdin handling**: True multi-turn stdin with long-running process
2. **Tool result visibility**: Show everything (controlled by `-v` flag)
3. **Error handling**: Report with color_eyre context and exit
4. **Session file format**: Custom bundle format (conversation.jsonl + patches + manifest)
5. **Concurrent access**: Fork sessions to allow parallel exploration

## Resources

- [Claude Code CLI Reference](https://code.claude.com/docs/en/cli-reference)
- [Headless Mode Documentation](https://code.claude.com/docs/en/headless)
