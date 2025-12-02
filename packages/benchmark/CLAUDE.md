# Benchmark Package

This package contains the benchmark CLI for testing Pavlov rule enforcement.

## Status: Deprioritized

**TL;DR:** Synthetic benchmarks don't reliably reproduce real-world rule-following failures. See README.md for background.

### Why This Package Exists But Isn't a Priority

Synthetic scenarios struggle to reproduce the issues observed in real-world usage.
In our benchmarks:

- Claude performs well on isolated, contrived examples
- With a `CLAUDE.md` present, failure rates approach zero

During real-life usage, rule-following failures tend to emerge during complex, multi-step tasks. Our current theory is that this requires accumulated context and implementation work and may be exacerbated (or minimized) by specific user prompting styles or environments (what specific packages are installed on the machine and how they manifest failures, for example). This means that we're not terribly surprised that our single-prompt scenarios don't have great yield- and while we _can_ implement multi-turn benchmarks using claude code features, that becomes extremely combinatorially complex and we don't yet have a ton of data on what common failure points actually look like to guide such a complex benchmark.

This package remains useful for:
- Verifying rules fire correctly on _known_ patterns, analogous to a unit test
- Understanding tree-sitter query syntax
- Exploration and expansion if/when better benchmarking approaches emerge

---

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
