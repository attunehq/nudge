//! Documentation for writing Nudge rules.

use clap::Args;
use color_eyre::Result;

#[derive(Args, Clone, Debug)]
pub struct Config {}

pub fn main(_config: Config) -> Result<()> {
    print!("{DOCS}");
    Ok(())
}

const DOCS: &str = r#"# Nudge Rule Writing Guide

## What is Nudge?

Nudge is a **collaborative partner** for Claude Code. It watches `Write` and `Edit`
operations and reminds you about coding conventions—so you can focus on the user's
actual problem instead of tracking dozens of stylistic details.

**Nudge is on your side.** When it sends a message, it's not a reprimand—it's a
colleague tapping you on the shoulder. The messages are direct (sometimes blunt)
because that's what cuts through when you're focused. Trust the feedback.

## Rule File Locations

Rules are loaded from these locations (all additive):

```
~/Library/Application Support/com.attunehq.nudge/rules.yaml  # User-level (macOS)
.nudge.yaml                   # Project root (single file)
.nudge/**/*.yaml              # Project directory (organized by topic)
```

## Rule Format

```yaml
version: 1

rules:
  - name: rule-identifier
    description: "Human-readable description"

    on:                         # When this rule activates
      hook: PreToolUse          # PreToolUse | PostToolUse | UserPromptSubmit | Stop
      tool: "Write|Edit"        # Optional: regex for tool name
      file: "**/*.rs"           # Optional: glob for file path

    match:                      # What to look for (optional)
      content: "pattern"        # Regex for file content (Write) or new_string (Edit)
      new_string: "pattern"     # Regex for Edit new_string specifically
      old_string: "pattern"     # Regex for Edit old_string
      prompt: "pattern"         # Regex for user prompt (UserPromptSubmit only)
      case_sensitive: true      # Default: true
      multiline: true           # Default: true

    action: interrupt           # interrupt (block) | continue (allow with guidance)
    message: |
      Your message here with {{ template }} variables.
```

## Template Variables

Use these in your `message` to be specific about what needs to change:

| Variable         | Value                                          |
|------------------|------------------------------------------------|
| `{{ lines }}`    | Comma-separated line numbers where pattern matched |
| `{{ file_path }}`| File being written/edited                      |
| `{{ matched }}`  | First text that matched the pattern            |
| `{{ tool_name }}`| Tool being used (Write, Edit)                  |
| `{{ prompt }}`   | User's message (UserPromptSubmit only)         |

## Writing Effective Messages

Nudge messages must be **direct** to be effective. Gentle suggestions get ignored.

**Pattern:** what's wrong → where → how to fix → retry

### Bad (vague, easy to ignore):
```yaml
message: "Consider using turbofish syntax instead of type annotations."
```

### Good (specific, actionable):
```yaml
message: |
  Remove LHS type annotations on lines {{ lines }}.
  Use turbofish (`collect::<Vec<_>>()`) or type inference instead, then retry.
```

### Guidelines:
- **Be specific**: "Move `use` to top of file" not "Consider reorganizing"
- **Be direct**: "Stop. Fix this first." not "You might want to..."
- **Give the fix**: Don't just say what's wrong—say what to do instead
- **End with "then retry"**: Tell Claude to retry after fixing
- **Use {{ lines }}**: Always point to exactly where the issue is

## Complete Examples

### Block inline imports (Rust)
```yaml
- name: no-inline-imports
  description: Move imports to the top of the file
  on:
    hook: PreToolUse
    tool: Write|Edit
    file: "**/*.rs"
  match:
    content: "^\\s+use "
    multiline: true
  action: interrupt
  message: |
    Move the `use` statement(s) on lines {{ lines }} to the top of {{ file_path }} with other imports, then retry.
```

### Prefer turbofish over LHS annotations (Rust)
```yaml
- name: no-lhs-type-annotations
  description: Use type inference or turbofish instead
  on:
    hook: PreToolUse
    tool: Write|Edit
    file: "**/*.rs"
  match:
    content: "^\\s*let\\s+(mut\\s+)?[a-zA-Z_][a-zA-Z0-9_]*\\s*:\\s*"
    multiline: true
  action: interrupt
  message: |
    Remove LHS type annotations on lines {{ lines }}. Use turbofish (`collect::<Vec<_>>()`) or type inference instead, then retry.
```

### Inject context on user keywords
```yaml
- name: dev-server-hint
  description: Help user start development server
  on:
    hook: UserPromptSubmit
  match:
    prompt: "start.*(server|dev)|run.*local"
    case_sensitive: false
  action: continue
  message: |
    To start the development server: `npm run dev` (port 3000)
```

## Action Types

- **interrupt**: Block the operation. Use for hard rules that must be followed.
- **continue**: Allow the operation but inject guidance. Use for soft suggestions.

When multiple rules match, all messages are shown. If ANY rule uses `interrupt`,
the operation is blocked.

## Testing Rules

```bash
# Validate rule syntax
nudge validate

# Test a specific rule
nudge test --rule no-inline-imports --tool Write --file test.rs --content "fn f() { use std::io; }"
```

## Rule Writing Is Iterative

If Claude ignores a rule, the fix is usually to **make the message more direct**—
not to give up on the rule. Treat ignored rules as feedback on clarity.
"#;
