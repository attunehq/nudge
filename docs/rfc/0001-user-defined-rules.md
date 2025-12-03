# RFC 0001: User-Defined Rules

## Summary

Add support for user-defined rules that can be specified in configuration files (YAML, TOML, JSON, JSONC). This enables users to customize Nudge's behavior without modifying Rust code.

## Motivation

Currently, all Nudge rules are hardcoded in `src/rules.rs`. This has several limitations:

1. **No customization**: Users can't add project-specific rules
2. **No personalization**: Users can't add personal coding preferences
3. **Requires Rust knowledge**: Adding rules requires Rust programming
4. **No runtime configuration**: Rules can only change via recompilation

User-defined rules enable:

- Project teams to enforce shared conventions via committed config files
- Individual developers to add personal preferences via user-level config
- Quick iteration on rules without rebuilding
- "Prompt injection" rules that inject helpful context when users mention keywords

## Design

### Config File Locations

Nudge searches for rule files in YAML format. All sources are additive:

| Location | Purpose |
|----------|---------|
| `~/Library/Application Support/com.attunehq.nudge/rules.yaml` | User-level rules (personal preferences) |
| `.nudge.yaml` | Project rules in single file |
| `.nudge/**/*.yaml` | Project rules in individual files |

**Loading order** (all additive):
1. User-level rules from `ProjectDirs::config_dir()/rules.yaml` if it exists
2. `.nudge.yaml` if it exists
3. `.nudge/` directory walked recursively in stable order (use `walkdir::WalkDir`'s sorted walking), loading all `*.yaml` files

All rules from all sources are collected into a single list, where the list is ordered by (1) the order in which the file was loaded and (2) the order of the rule within the file. In other words, the rules in each distinct file "extend" the global pool of rules when loaded. Multiple rules with the same name are allowed, but they emit a `tracing::warn!` at startup.

**Evaluation**: All matching rules fire. If multiple rules match:
- Messages from all rules are concatenated with `\n\n---\n\n` between them.
- If any matched rule is `interrupt`, the overall operation is `interrupt`.

### Rule Schema

```yaml
# .nudge/rules.yaml
version: 1

rules:
  - name: no-console-log
    description: Prevent console.log in production code

    # When does this rule activate?
    on:
      hook: PreToolUse           # PreToolUse | PostToolUse | UserPromptSubmit | Stop
      tool: Write|Edit           # regex, only for PreToolUse/PostToolUse, otherwise ignored
      file: "**/*.ts"            # glob pattern for relative file path (from project root) to further constrain activation

    # What triggers the rule?
    match:
      content: "console\\.log"   # regex to search in message/tool call content

    # What happens when triggered?
    action: interrupt            # interrupt | continue
    message: |
      Remove console.log statements. Use a proper logger instead.
      Found on lines: {{ lines }}
```

### Schema Details

#### `on` - Activation Criteria

Determines when the rule is even considered for evaluation.

```yaml
on:
  hook: PreToolUse         # Required. Which hook event type.
  tool: "Write|Edit"       # Optional. Regex for tool_name. Only for *ToolUse hooks.
  file: "**/*.ts"          # Optional. Glob for file_path. Only for tools with file_path.
```

**Hook types:**

| Hook | Description | Available Fields |
|------|-------------|------------------|
| `PreToolUse` | Before tool execution | `tool_name`, `tool_input` (file_path, content, etc.) |
| `PostToolUse` | After tool execution | Same as PreToolUse, plus `tool_response` |
| `UserPromptSubmit` | When user sends a message | `user_prompt` |
| `Stop` | When agent finishes | `stop_reason`, `assistant_message` |

**Tool filtering** (regex):
- `Write` - matches Write tool only
- `Write|Edit` - matches Write or Edit
- `.*` - matches any tool
- `Bash.*` - matches Bash, BashOutput, etc.

**File filtering** (glob patterns):
- `**/*.rs` - all Rust files
- `src/**/*.ts` - TypeScript files in src/
- `!**/test/**` - negation (exclude test directories)

#### `match` - Content Matching

Determines whether the rule fires based on content inspection.

```yaml
match:
  # For PreToolUse/PostToolUse with Write tool:
  content: "pattern"       # Regex to match in file content

  # For PreToolUse/PostToolUse with Edit tool:
  new_string: "pattern"    # Regex to match in new_string (content being written)
  old_string: "pattern"    # Regex to match in old_string (content being replaced)

  # For UserPromptSubmit:
  prompt: "pattern"        # Regex to match in user's message

  # For Stop:
  message: "pattern"       # Regex to match in assistant's final message

  # Common options:
  case_sensitive: true     # Default: true
  multiline: true          # Default: true (^ and $ match line boundaries)
```

**Note**: If unspecified, `match` patterns default to `.*`, meaning that they always match by default.

#### `action` - Response Type

```yaml
action: interrupt    # Block the operation
action: continue     # Allow but inject guidance. Note that the model may use that guidance to inform future operations, but will not modify the action that was just taken based on this guidance.
```

#### `message` - Response Message

The message supports template interpolation:

```yaml
message: |
  Found issues on lines: {{ lines }}
  File: {{ file_path }}
  Matched text: {{ matched }}
```

**Available variables:**

| Variable | Description | Availability |
|----------|-------------|--------------|
| `{{ lines }}` | Comma-separated line numbers | When `content`/`new_string` matches |
| `{{ file_path }}` | The file being written/edited | PreToolUse with file_path |
| `{{ matched }}` | The matched text (first match) | Any regex match |
| `{{ tool_name }}` | Name of the tool | *ToolUse hooks |
| `{{ prompt }}` | User's message | UserPromptSubmit |

### Example Rules

#### 1. Block TODO comments

```yaml
rules:
  - name: no-todos
    description: Don't allow TODO comments to be written
    on:
      hook: PreToolUse
      tool: Write|Edit
    match:
      content: "TODO|FIXME|XXX"
    action: interrupt
    message: |
      Resolve TODO/FIXME/XXX comments before committing.
      Found on lines: {{ lines }}
```

#### 2. Remind about test conventions

```yaml
rules:
  - name: test-naming
    description: Remind about test file naming
    on:
      hook: PreToolUse
      tool: Write
      file: "**/*_test.go"
    action: continue  # Soft reminder, don't block
    message: |
      Remember: Go test functions must be named TestXxx (capital T, capital X).
      Table-driven tests are preferred for multiple cases.
```

#### 3. Inject context on user keywords (UserPromptSubmit)

```yaml
rules:
  - name: server-start-hint
    description: Help user start the dev server
    on:
      hook: UserPromptSubmit
    match:
      prompt: "start.*(server|dev)|run.*server"
      case_sensitive: false
    action: continue
    message: |
      To start the development server:
      ```
      npm run dev
      ```
      The server runs on http://localhost:3000
```

#### 4. Enforce import style

```yaml
rules:
  - name: no-relative-imports
    description: Use absolute imports, not relative
    on:
      hook: PreToolUse
      tool: Write|Edit
      file: "**/*.ts"
    match:
      content: "from ['\"]\\.\\./"
    action: interrupt
    message: |
      Use absolute imports instead of relative imports.
      Change `from '../...'` to `from '@/...'`
      Found on lines: {{ lines }}
```

#### 5. Warn on large files

```yaml
rules:
  - name: large-file-warning
    description: Warn when creating large files
    on:
      hook: PreToolUse
      tool: Write
    match:
      content: "(?s).{10000,}"  # More than 10k characters
    action: continue
    message: |
      This file is quite large (>10k chars). Consider splitting it into smaller modules.
```

#### 6. Single-rule file (in `.nudge/` directory)

```yaml
# .nudge/no-console.yaml
version: 1

rules:
  - name: no-console-log
    description: Prevent console.log in production code
    on:
      hook: PreToolUse
      tool: Write|Edit
      file: "**/*.ts"
    match:
      content: "console\\.(log|debug|info)"
    action: interrupt
    message: |
      Remove console.log statements. Use a proper logger instead.
```

### Config Validation

Add a new CLI command to validate rule configs:

```bash
# Validate all discoverable config files
nudge validate

# Validate a specific file
nudge validate .nudge/rules.yaml
```

Output on success:
```
.nudge/rules.yaml: 5 rules loaded
  - no-console-log (PreToolUse, interrupt)
  - test-naming (PreToolUse, continue)
  - server-start-hint (UserPromptSubmit, continue)
  - no-relative-imports (PreToolUse, interrupt)
  - large-file-warning (PreToolUse, continue)
```

Validation errors are bubbled up from serde to the user via `color_eyre`, so the output format is not exactly known at this time but will likely include context and suggestions for the issue.

### Rule Testing

Add a command to test rules against sample input:

```bash
# Test with inline content
nudge test --rule no-console-log --tool Write --file app.ts --content 'console.log("hi")'

# Test with file
nudge test --rule no-console-log --tool Write --file app.ts --content-file /path/to/sample.ts

# Test user prompt rule
nudge test --rule server-start-hint --prompt "how do I start the server?"
```

Output:
```
Rule: no-console-log
Result: INTERRUPT
Message:
  Remove console.log statements. Use a proper logger instead.
  Found on lines: 1
```

## Implementation

### Module Structure

```
src/
├── rules.rs             # Public API: evaluate_all(), RuleRegistry
├── rules/
│   ├── config.rs        # Config file discovery and YAML parsing
│   ├── eval.rs          # Rule evaluation and response aggregation
│   └── schema.rs        # Rule schema types
├── cmd/
│   ├── validate.rs      # New: nudge validate
│   └── test.rs          # New: nudge test
examples/
├── rules/
│   └── rust.yaml        # Example rules (current rules, rewritten as YAML)
```

### Key Types

```rust
// src/rules/schema.rs

#[derive(Debug, Deserialize)]
pub struct RuleConfig {
    pub version: MustBe!(1),
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
pub struct Rule {
    pub name: String,
    pub description: Option<String>,
    pub on: Activation,
    pub r#match: Option<Match>,
    pub action: Action,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct Activation {
    pub hook: HookType,
    pub tool: Option<String>,      // Regex pattern
    pub file: Option<String>,      // Glob pattern
}

#[derive(Debug, Deserialize)]
pub enum HookType {
    PreToolUse,
    PostToolUse,
    UserPromptSubmit,
    Stop,
}

#[derive(Debug, Deserialize)]
pub struct Match {
    pub content: Option<String>,
    pub new_string: Option<String>,
    pub old_string: Option<String>,
    pub prompt: Option<String>,
    pub message: Option<String>,
    #[serde(default = "true")]
    pub case_sensitive: bool,
    #[serde(default = "true")]
    pub multiline: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Interrupt,
    Continue,
}
```

### Rule Registry

```rust
// src/rules/mod.rs

pub struct RuleRegistry {
    rules: Vec<CompiledRule>,  // All rules: user + project
}

impl RuleRegistry {
    pub fn new() -> Result<Self> {
        let mut rules = vec![];
        let mut seen_names: HashSet<String> = HashSet::new();

        // Load all rules from all sources (purely additive)
        let all_rules = config::load_all_rules()?;

        for rule in all_rules {
            // Warn on duplicate names (but still add the rule)
            if !seen_names.insert(rule.name.clone()) {
                tracing::warn!("Multiple rules with name '{}' found", rule.name);
            }
            rules.push(rule);
        }

        Ok(Self { rules })
    }

    pub fn evaluate(&self, hook: &Hook) -> Response {
        let mut messages: Vec<String> = vec![];
        let mut any_interrupt = false;

        // Evaluate ALL matching rules
        for rule in &self.rules {
            if let Some(result) = rule.evaluate(hook) {
                messages.push(result.message);
                if result.is_interrupt {
                    any_interrupt = true;
                }
            }
        }

        if messages.is_empty() {
            return Response::Passthrough;
        }

        // Concatenate all messages
        let combined_message = messages.join("\n\n---\n\n");

        // If ANY rule interrupted, the whole response interrupts
        if any_interrupt {
            Response::Interrupt(InterruptResponse::builder()
                .r#continue(false)
                .system_message(combined_message)
                .build())
        } else {
            Response::Continue(ContinueResponse::builder()
                .r#continue(true)
                .system_message(combined_message)
                .build())
        }
    }
}
```

### Config Loading

```rust
// src/rules/config.rs

pub fn load_all_rules() -> Result<Vec<CompiledRule>> {
    let mut rules = vec![];

    // 1. User-level rules
    if let Some(proj_dirs) = ProjectDirs::from("com", "attunehq", "nudge") {
        let path = proj_dirs.config_dir().join("rules.yaml");
        rules.extend(load_rules_from_file(&path)?);
    }

    // 2. Project-level single file
    rules.extend(load_rules_from_file(Path::new(".nudge.yaml"))?);

    // 3. Project-level directory (walk all .yaml files)
    let nudge_dir = Path::new(".nudge");
    if nudge_dir.is_dir() {
        for entry in walkdir::WalkDir::new(nudge_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some("yaml".as_ref()))
        {
            rules.extend(load_rules_from_file(entry.path())?);
        }
    }

    Ok(rules)
}

fn load_rules_from_file(path: &Path) -> Result<Vec<CompiledRule>> {
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(path)?;
    let config: RuleConfig = serde_yaml::from_str(&content)?;

    config.rules
        .into_iter()
        .map(|rule| CompiledRule::compile(rule))
        .collect()
}
```

### Compiled Rules

Rules are compiled at load time for efficient evaluation:

```rust
// src/rules/eval.rs

pub struct CompiledRule {
    pub name: String,
    pub hook_type: HookType,
    pub tool_pattern: Option<Regex>,
    pub file_glob: Option<GlobPattern>,
    pub content_pattern: Option<Regex>,
    pub prompt_pattern: Option<Regex>,
    pub action: Action,
    pub message_template: String,
}

impl CompiledRule {
    pub fn evaluate(&self, hook: &Hook) -> Option<Response> {
        // 1. Check hook type matches
        if !self.matches_hook_type(hook) {
            return None;
        }

        // 2. Check tool name (if specified)
        if let Some(ref pattern) = self.tool_pattern {
            if !self.matches_tool(hook, pattern) {
                return None;
            }
        }

        // 3. Check file path (if specified)
        if let Some(ref glob) = self.file_glob {
            if !self.matches_file(hook, glob) {
                return None;
            }
        }

        // 4. Check content patterns
        let match_result = self.check_content_match(hook)?;

        // 5. Build response
        let message = self.render_message(&match_result);
        Some(self.build_response(message))
    }
}
```

## Migration Path

1. **Convert existing Rust rules to YAML**: The five existing rules in `src/rules.rs` will be rewritten as `examples/rules/rust.yaml`. The Rust implementations will be removed.

2. **Implement rule loading**: Add YAML config parsing and file discovery.

3. **Implement evaluation**: Replace `evaluate_all()` with `RuleRegistry` that aggregates all matching rules.

4. **Add CLI commands**: `nudge validate` and `nudge test` for rule development.

## Alternatives Considered

### 1. Lua/Rhai scripting

**Pros**: Maximum flexibility, Turing-complete rules
**Cons**: Complexity, security concerns, harder to validate, steeper learning curve

**Decision**: Start with declarative config. Scripting could be added later if needed.

### 2. Multiple config formats (YAML, TOML, JSON)

**Pros**: Meet user preferences
**Cons**: More complexity, more dependencies

**Decision**: Start with YAML only. It's readable, supports comments, and is widely used for config. Can add other formats later if needed.

### 3. Rule DSL

**Pros**: Optimized for rule expression
**Cons**: Another language to learn, tooling needed

**Decision**: Use standard config formats with good schema documentation.

## Design Decisions

1. **No hardcoded Rust rules**: All rules are declarative YAML. The existing Rust rules are converted to example YAML files that users can copy into their projects. This keeps the rule format uniform and makes rules inspectable.

2. **YAML only**: Start simple with one format. YAML is readable, supports comments, and is widely understood. Other formats can be added later if there's demand.

3. **Purely additive**: Rules are loaded from all sources and combined into a single list. There's no override or disable mechanism. This mirrors how Claude context works - everything is additive. Duplicate rule names trigger a warning but both rules still fire.

4. **No logical operators in match**: Users can implement AND logic using regex (e.g., `(?=.*pattern1)(?=.*pattern2)`). For OR logic, users define multiple rules. This keeps the schema simple.

5. **All matching rules fire**: Rather than first-match-wins, all rules that match are evaluated and their messages concatenated. If any rule returns `interrupt`, the operation is blocked. This allows multiple rules to provide feedback simultaneously.

6. **Flexible project structure**: Projects can use `.nudge.yaml` for a single file, `.nudge/*.yaml` for organized rule files, or both. File names don't matter - only the content.

## Appendix: Full Schema Reference

```yaml
# .nudge/rules.yaml (or .nudge.yaml, or .nudge/*.yaml)

# Schema version (required)
version: 1

# List of rules (required)
rules:
  - # Rule identifier (required)
    name: string

    # Human-readable description (optional)
    description: string

    # Activation criteria (required)
    on:
      # Hook event type (required)
      # Values: PreToolUse, PostToolUse, UserPromptSubmit, Stop
      hook: HookType

      # Tool name pattern - regex (optional, only for *ToolUse hooks)
      # Examples: "Write", "Write|Edit", "Bash.*"
      tool: string

      # File path pattern - glob (optional, only for tools with file_path)
      # Examples: "**/*.rs", "src/**/*.ts"
      file: string

    # Content matching criteria (optional)
    # If omitted, rule fires on activation alone
    match:
      # For Write tool content (regex)
      content: string

      # For Edit tool (regex)
      new_string: string
      old_string: string

      # For UserPromptSubmit (regex)
      prompt: string

      # For Stop (regex)
      message: string

      # Regex options
      case_sensitive: bool  # default: true
      multiline: bool       # default: true

    # Response type (required)
    # Values: interrupt (block), continue (allow with guidance)
    action: Action

    # Response message (required)
    # Supports template variables: {{ lines }}, {{ file_path }}, {{ matched }}, etc.
    message: string
```

## References

- [Claude Code Hooks Documentation](https://docs.anthropic.com/en/docs/claude-code/hooks)
- [Example Nudge rules](../examples/rules/rust.yaml)
