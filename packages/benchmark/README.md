# Benchmark

A harness for evaluating Pavlov rule effectiveness through synthetic scenarios.

## Background

Pavlov's core thesis is that coding agents (like Claude Code) struggle to follow project-specific guidelines because guidance in context files competes with accumulated task context. As conversations grow, rules slip. The [RESEARCH.md](../../docs/RESEARCH.md) file documents this phenomenon: users report ~95% rule compliance in initial messages degrading to ~20-60% after 10+ exchanges, with rules often forgotten entirely after auto-compaction.

This benchmark package was built to test whether Pavlov- which enforces rules outside the standard context loop via hooks- actually improves adherence. The goal was to produce quantitative evidence: "with Pavlov, rule X is followed Y% more often."

## What We Built

The harness runs scenarios that:

1. Set up a temporary project with starter code
2. Provide guidance (either via CLAUDE.md, Pavlov hooks, or nothing)
3. Give Claude a task prompt
4. Evaluate the result using tree-sitter queries to detect rule violations

Scenarios target specific patterns drawn from real GitHub issues: LHS type annotations vs turbofish, import placement, comment discipline, assertion library usage, and similar.

See [scenarios/README.md](scenarios/README.md) for details on scenario structure and how to write them.

## What We Found

The scenarios did not reliably reproduce the issues we observe in real-world usage.

Even with prompts carefully designed to trigger known problematic behaviors:

- Claude performs well on isolated, contrived examples
- With a CLAUDE.md file present, failure rates approach zero
- The rule-following failures we experience in practice tend to emerge during complex, multi-step tasks with accumulated context- not in fresh, single-prompt scenarios

This creates a fundamental challenge: the behaviors worth measuring are precisely those that resist synthetic reproduction. They require the kind of context accumulation and task complexity that's difficult to simulate in a benchmark.

## Current Status

This package is deprioritized. The infrastructure remains useful for:

- Verifying rules fire correctly on _known_ patterns, analogous to a unit test
- Understanding tree-sitter query syntax
- Exploration and expansion if/when better benchmarking approaches emerge

For demonstrating Pavlov's value, we're instead focusing on real-world usage data: capturing statistics about interventions during actual development, which provides organic evidence that's harder to dismiss as benchmark gaming.

## Usage

```bash
# List available scenarios
cargo run -p benchmark -- list

# Run benchmarks
cargo run -p benchmark -- run

# Inspect syntax tree for debugging queries
cargo run -p benchmark -- syntax -l rust 'let x: i32 = 5;'
```
