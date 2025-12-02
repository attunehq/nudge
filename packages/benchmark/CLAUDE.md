# Benchmark Package

This package contains the benchmark CLI for testing Pavlov rule enforcement.

## Debugging Tree-sitter Queries

When writing or debugging scenario queries, use the `syntax` subcommand to inspect how code is parsed:

```bash
# Parse literal code
cargo run -p benchmark -- syntax -l rust 'fn foo() -> i32 { 42 }'

# Parse a file
cargo run -p benchmark -- syntax -l rust path/to/file.rs
```

This outputs the full syntax tree with node kinds (what you match in queries) and field names (like `name:`, `body:`). Use this to understand the tree structure when your queries aren't matching as expected.

## Writing Scenarios

See `scenarios/README.md` for comprehensive guidance on writing benchmark scenarios, including:
- Scenario TOML structure
- Tree-sitter query patterns
- Common pitfalls and best practices
