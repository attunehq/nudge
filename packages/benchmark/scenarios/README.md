# Writing Benchmark Scenarios

This guide explains how to write effective benchmark scenarios that meaningfully test whether an LLM follows coding guidelines.

## Scenario Structure

Each scenario is a TOML file with the following top-level fields:

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique identifier for the scenario |
| `description` | No | Human-readable description of what the scenario tests |
| `guidance` | Yes | Content for the agent's context file (e.g., `CLAUDE.md`). Use this for all guidance rather than setup commands, so evaluators can test with/without guidance. |
| `prompt` | Yes | The task prompt given to the agent |
| `commands` | Yes | Array of setup commands to create the test environment |
| `expected` | Yes | Array of evaluation commands to verify the result |

### Setup Commands (`[[commands]]`)

Setup commands create the test environment in a temporary directory before the agent runs.

#### `write` - Create or overwrite a file

```toml
[[commands]]
type = "write"

[commands.content]
path = "src/lib.rs"
content = """
fn hello() {}
"""
```

Creates the file at `path` with the given `content`. Parent directories are created automatically. Overwrites if the file exists.

#### `append` - Append to a file

```toml
[[commands]]
type = "append"

[commands.content]
path = "src/lib.rs"
content = """
fn goodbye() {}
"""
separator = "\n\n"
```

Appends `content` to the file at `path`. If the file exists and `separator` is set, inserts the separator between existing content and new content. Creates the file if it doesn't exist.

#### `command` - Run a shell command

```toml
[[commands]]
type = "command"

[commands.content]
binary = "cargo"
args = ["init"]
```

Runs the specified binary with arguments in the project directory. Useful for initializing projects or cloning repositories:

```toml
[[commands]]
type = "command"

[commands.content]
binary = "git"
args = ["clone", "https://github.com/user/repo.git", "."]
```

### Evaluation Commands (`[[expected]]`)

Evaluation commands verify the final state after the agent runs. The scenario passes only if all evaluations pass.

#### `exists` - Verify a pattern matches

```toml
[[expected]]
type = "exists"

[expected.content]
path = "src/lib.rs"

[expected.content.matcher]
language = "rust"
query = '''
(function_item
  name: (identifier) @name
  (#eq? @name "my_function"))
'''
```

Passes if the tree-sitter query matches at least once in the file(s). The `path` supports glob patterns:
- `src/lib.rs` - specific file
- `src/*.rs` - all `.rs` files in `src/`
- `src/**/*.rs` - all `.rs` files under `src/` recursively

#### `not_exists` - Verify a pattern does NOT match

```toml
[[expected]]
type = "not_exists"

[expected.content]
path = "src/**/*.rs"

[expected.content.matcher]
language = "rust"
query = '''
(let_declaration type: (_) @type)
'''
```

Passes if the tree-sitter query matches zero times across all matched files.

#### `command` - Verify a command succeeds

```toml
[[expected]]
type = "command"

[expected.content]
binary = "cargo"
args = ["build"]
```

Passes if the command exits with code 0. Useful for verifying code compiles or tests pass.

### The `between` Filter

For advanced matching, you can filter results based on text between two captures. This validates content that exists between AST nodes but isn't directly queryable (e.g. whitespace).

```toml
[[expected]]
type = "not_exists"

[expected.content]
path = "src/lib.rs"

[expected.content.matcher]
language = "rust"
query = '''
((field_declaration) @field1 . (field_declaration) @field2)
'''

[expected.content.between]
from = "field1"
to = "field2"
not_contains = "\n\n"
```

| Field | Description |
|-------|-------------|
| `from` | Label of the capture where the region starts (text starts at end of this capture) |
| `to` | Label of the capture where the region ends (text ends at start of this capture) |
| `contains` | If set, only matches where the between-text contains this string pass |
| `not_contains` | If set, only matches where the between-text does NOT contain this string pass |

## Key Principles

### 1. The Task Must Genuinely Require the Tested Behavior

The most common mistake is writing a task that can be completed correctly without exercising the pattern you're testing. If you're testing that models use turbofish instead of LHS type annotations, design a task where type inference genuinely fails and explicit types are required.

**Bad:** A simple task where inference works fine (model might use turbofish by coincidence)

**Good:** A task returning `Box<dyn Trait>` where the compiler cannot infer concrete types

See `lhs_annotations.toml` for an example where we use `Box<dyn Config>` to break inference and force explicit type specification.

### 2. Always Pair "not_exists" with "exists" Checks

When testing that something bad does NOT appear, you must also verify the model actually attempted the task. Otherwise, a model that writes nothing would pass by default.

```toml
# First: verify they actually did something
[[expected]]
type = "exists"

[expected.content]
path = "src/lib.rs"

[expected.content.matcher]
language = "rust"
query = '''
(function_item
  name: (identifier) @name
  (#eq? @name "my_function")
  body: (block
    (let_declaration) @let))
'''

# Then: verify they didn't do the bad thing
[[expected]]
type = "not_exists"

[expected.content]
path = "src/lib.rs"

[expected.content.matcher]
language = "rust"
query = '''
(let_declaration type: (_) @type)
'''
```

### 3. Guard Against Reward Hacking

Models may find creative shortcuts that technically satisfy the test without following the spirit of the rules. Add checks that existing code wasn't modified in ways that trivialize the task.

```toml
# Ensure the model didn't modify existing struct to bypass the challenge
[[expected]]
type = "exists"

[expected.content]
path = "src/lib.rs"
[expected.content.matcher]
language = "rust"
query = '''
((attribute_item
  (attribute
    (identifier) @attr
    (#eq? @attr "derive")
    arguments: (token_tree) @args
    (#match? @args "Deserialize")))
.
(struct_item
  name: (type_identifier) @name
  (#eq? @name "OriginalStruct")))
'''
```

**Even better:** Design tasks that are structurally immune to shortcuts.

### 4. Use Explicit Captures in Tree-sitter Queries

Tree-sitter only provides span information for nodes with explicit `@name` captures. Without captures, matches are detected but cannot be highlighted in output, making it difficult to tell where the problem was found.

**Bad:** `(let_declaration type: (_))`

**Good:** `(let_declaration type: (_) @type)`

### 5. Use Triple-Quote Syntax for Queries

Use `'''` for query strings to avoid escaping issues and enable readable multiline queries:

```toml
query = '''
(function_item
  name: (identifier) @name
  (#eq? @name "parse_config")
  body: (block
    (let_declaration) @let))
'''
```

### 6. Write Clear, Specific Prompts

The prompt should unambiguously describe the task. Include:
- What to implement
- Any constraints ("do not modify existing code")
- Example input/output if relevant

Avoid prompts so vague that a correct solution might not trigger the tested pattern.

### 7. Provide Realistic Starter Code

The starter code should:
- Compile (or be close to compiling)
- Include enough context that the model understands the codebase
- Set up the situation where the tested pattern becomes relevant

One somewhat convenient way to do this is to use an existing codebase (e.g. an open source project) as your starter code, especially if it has a lot of patterns where you want to test the ability of the model to follow your instructions in the face of lots of examples to the contrary.

## Tree-sitter Query Tips

### Debugging Queries with the `syntax` Subcommand

When queries aren't matching as expected, use the `syntax` subcommand to inspect the actual tree structure:

```bash
# Parse literal code to see its tree structure
cargo run -p benchmark -- syntax -l rust 'let x: i32 = 5;'

# Parse a file
cargo run -p benchmark -- syntax -l rust path/to/file.rs
```

This shows the full syntax tree with node kinds and field names, helping you understand exactly what patterns to match.

### Adjacent Sibling Matching

Use `.` to match immediately adjacent siblings (useful for checking attributes on items):

```
((attribute_item ...) . (struct_item ...))
```

Note: This only matches the immediately preceding sibling. If there are multiple stacked attributes, you may need multiple checks.

### Checking for Patterns Inside Specific Contexts

To check for a pattern only within a specific function or block:

```
(function_item
  name: (identifier) @fn_name
  (#eq? @fn_name "target_function")
  body: (block
    (pattern_to_find) @match))
```

### Using Predicates

- `#eq?` - exact string match
- `#match?` - regex match (useful for checking if something contains a substring)

```
(#eq? @name "exact_name")
(#match? @args "Deserialize")
```

## Reference Implementation

See `lhs_annotations.toml` as the gold standard example demonstrating all these principles:
- Task designed to genuinely require the tested behavior (`Box<dyn Trait>` breaks inference)
- Positive existence check (function contains `let` declarations)
- Negative check for the forbidden pattern (no LHS type annotations)
- Guards against reward hacking (original structs unchanged)
- Proper captures in all queries
- Clear, specific prompt with constraints
