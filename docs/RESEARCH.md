# Research: CLAUDE.md Pain Points and Example Repositories

This document captures research on real-world CLAUDE.md usage patterns, common failure modes, and example repositories that could inform benchmark scenario development.

## Problem Validation

The problems described in [PLAN.md](PLAN.md) are widely reported in the Claude Code community. Key GitHub issues confirm:

- Users report **~95% rule compliance in first messages → ~20-60% after 10+ exchanges**
- Rules are followed initially but degrade as conversation length increases
- After auto-compaction, rules are often **completely forgotten**
- Claude can correctly recite instructions when asked but ignores them in practice

> "Claude Code has enormous potential - but it is currently akin to a senior developer with the attention span of a three-year-old"
> - [Issue #7083](https://github.com/anthropics/claude-code/issues/7083)

## Most Commonly Ignored Rule Types

| Rule Type          | Example Rule                    | Observed Behavior                                         |
|--------------------|---------------------------------|-----------------------------------------------------------|
| Comment discipline | "No comments unless necessary"  | Claude adds comments anyway, acknowledges rule when asked |
| Import placement   | "Never import inside functions" | Imports in functions, adds `# noqa` to suppress lint      |
| Lint suppression   | "DO NOT disable lint rules"     | Adds `# noqa` or equivalent comments                      |
| Security rules     | "NEVER commit API keys"         | Commits credentials to git                                |
| Commit formats     | "Use specific message format"   | Uses default format instead                               |
| Build procedures   | "Run X before Y"                | Skips steps or guesses commands                           |

### Sources
- [Issue #7083 - Import Restrictions Ignored](https://github.com/anthropics/claude-code/issues/7083)
- [Issue #2544 - Mandatory Rules Ignored](https://github.com/anthropics/claude-code/issues/2544)
- [Issue #2142 - Security Guidelines Ignored](https://github.com/anthropics/claude-code/issues/2142)
- [Issue #528 - Inconsistent Adherence](https://github.com/anthropics/claude-code/issues/528)

## Context Decay / Compaction Problem

The most frequently reported issue pattern:

1. Start session → rules followed
2. Work for a while → rules start slipping
3. Auto-compaction triggers → **rules completely forgotten**
4. User must manually say "re-read CLAUDE.md" to restore compliance, with varying results

### Key Issues
- [Issue #6354 - Forgets After Compaction](https://github.com/anthropics/claude-code/issues/6354)
- [Issue #11545 - Ignoring After Compact](https://github.com/anthropics/claude-code/issues/11545)
- [Issue #10006 - Forgets Everything](https://github.com/anthropics/claude-code/issues/10006)
- [Issue #4017 - /compact Causes Ignoring](https://github.com/anthropics/claude-code/issues/4017)

## Workarounds Users Have Tried

| Workaround                 | How It Works                                             | Effectiveness                 |
|----------------------------|----------------------------------------------------------|-------------------------------|
| Recursive self-display     | Rule says "display all rules at start of every response" | Effective but token-expensive |
| `/check-rules` command     | Manual refresh before tasks                              | Inconsistent                  |
| Separate RULES.md          | Split into multiple files                                | No improvement reported       |
| Enforcement agents         | Custom agent to check compliance                         | Complex setup                 |
| Manual "re-read CLAUDE.md" | After every compaction                                   | Works but tedious             |

Source: [DEV.to - Stop Claude From Forgetting Rules](https://dev.to/siddhantkcode/an-easy-way-to-stop-claude-code-from-forgetting-the-rules-h36)

---

## Example Repositories with CLAUDE.md Files

### Curated Collections

| Repository                                                                                  | Description                                                                        |
|---------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------|
| [josix/awesome-claude-md](https://github.com/josix/awesome-claude-md)                       | Curated collection of CLAUDE.md files from public projects, filterable by language |
| [steipete/agent-rules](https://github.com/steipete/agent-rules)                             | Reusable rules for AI assistants - commit standards, code quality, workflows       |
| [kariedo/claude-code-security-rules](https://github.com/kariedo/claude-code-security-rules) | Security rules for Python, JS, Java, PHP, Ruby, **Rust**, C                        |

### Rust-Specific Projects

| Repository                                                                                  | Key Rules/Patterns                                                                                          |
|---------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------|
| [hashintel/hash](https://github.com/hashintel/hash/blob/main/CLAUDE.md)                     | Large monorepo (Rust + TS). Branch naming (`<name>/h-XXXX-desc`), PR title formats, `cargo doc` conventions |
| [KentBeck/BPlusTree3](https://github.com/KentBeck/BPlusTree3/blob/main/rust/docs/CLAUDE.md) | TDD-focused. Red→Green→Refactor, Tidy First (separate structural vs behavioral commits)                     |
| [bredmond1019/claude-sdk-rs](https://github.com/bredmond1019/claude-sdk-rs)                 | Standard cargo conventions                                                                                  |
| [ruvnet/claude-flow Wiki](https://github.com/ruvnet/claude-flow/wiki/CLAUDE-MD-Rust)        | Comprehensive Rust template with cargo batching, memory safety patterns                                     |

### Notable Rule Categories from These Projects

**From hashintel/hash:**
- Branch naming: `<shortname>/h-XXXX-description`
- PR titles: `H-XXXX: Description`
- "Always critically evaluate and challenge user suggestions"
- Generate Rust docs with `cargo doc --no-deps --all-features`

**From KentBeck/BPlusTree3:**
- TDD cycle: Red → Green → Refactor
- Tidy First: separate structural changes from behavioral changes
- Never mix both in a single commit
- Only commit when all tests pass and warnings resolved

**From claude-flow Rust template:**
- Batch ALL cargo build/test/run commands
- `cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings`
- Memory safety patterns for borrowing/ownership

**From security-rules:**
- Injection prevention (SQL, Command, Code)
- No hardcoded secrets
- Path traversal protection
- Timing-safe comparisons

---

## Benchmark Scenario Ideas

Based on this research, high-value scenarios to test:

### Category: Comment Discipline
- Rule: "Only add comments when logic is non-obvious"
- Task: Implement a function with complex logic
- Test: Verify no unnecessary comments added

### Category: Import Placement (Python/JS/Rust)
- Rule: "All imports at top of file"
- Task: Add functionality requiring new imports
- Test: Verify imports not placed inside functions

### Category: Lint Rule Compliance
- Rule: "Never suppress lint warnings with noqa/allow"
- Task: Write code that triggers a lint warning
- Test: Verify warning is fixed, not suppressed

### Category: Security Patterns
- Rule: "Never hardcode secrets"
- Task: Implement API client that needs credentials
- Test: Verify credentials loaded from env/config, not hardcoded

### Category: Commit Message Format
- Rule: "Use conventional commits: type(scope): description"
- Task: Make changes and commit
- Test: Verify commit message matches format

### Category: Structural vs Behavioral Separation
- Rule: "Refactoring and feature work in separate commits"
- Task: "Rename X and add feature Y"
- Test: Verify two separate commits created

### Category: Build Command Adherence
- Rule: "Always run `cargo clippy` before committing"
- Task: Implement feature and commit
- Test: Verify clippy was run (could use hooks to detect)

---

## Open Questions for Further Research

1. **Which rule types have highest failure rates?** Need quantitative data.
2. **Does rule length/complexity correlate with adherence?** Some evidence suggests not.
3. **Are certain phrasings more effective?** "NEVER do X" vs "Always do Y" vs "Prefer Y over X"
4. **Do rules at the top of CLAUDE.md get followed more?** Position effects unclear.
5. **Does explicit reasoning help?** "Do X because Y" vs just "Do X"
