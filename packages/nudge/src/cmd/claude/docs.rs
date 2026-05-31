//! Documentation for writing Nudge rules.

use clap::Args;
use color_eyre::Result;
use color_print::cstr;

#[derive(Args, Clone, Debug)]
pub struct Config {}

pub fn main(_config: Config) -> Result<()> {
    println!("{DOCS}");
    Ok(())
}

const DOCS: &str = cstr!("\
<bold><blue>Nudge Config Writing Guide</blue></bold>

<bold>What is Nudge?</bold>

  Nudge is a <cyan>collaborative partner</cyan> for agent hooks. It supports Claude Code
  and Codex CLI, and reminds you about coding conventions when provider hooks expose
  Write, Edit, WebFetch, Bash, or prompt-submission surfaces.

  <green>Nudge is on your side.</green> When it sends a message, it's not a reprimand. It's
  a colleague tapping you on the shoulder. The messages are direct (sometimes
  blunt) because that's what cuts through when you're focused.

<bold>Config File Locations</bold>

  Rules and workflows are loaded from these locations (all additive):

    <cyan>$CONFIG_DIR/rules.yaml</cyan>        <dim>User-level rules</dim>
    <cyan>.nudge.yaml</cyan>                   <dim>Project root</dim>
    <cyan>.nudge/**/*.yaml</cyan>              <dim>Project directory</dim>

  <dim>$CONFIG_DIR by platform:</dim>
    <dim>Linux:</dim>   <cyan>~/.config/nudge</cyan>
    <dim>macOS:</dim>   <cyan>~/Library/Application Support/com.attunehq.nudge</cyan>
    <dim>Windows:</dim> <cyan>%APPDATA%\\attunehq\\nudge</cyan>

<bold>Rule Format</bold>

  <yellow>version: 1</yellow>

  <yellow>rules:</yellow>
    <yellow>- name: rule-identifier</yellow>
      <yellow>description: \"Human-readable description\"</yellow>  <dim># Optional</dim>
      <yellow>action: block</yellow>                         <dim># block (default) or substitute</dim>
      <yellow>message: \"Your message shown at each match\"</yellow> <dim># Required for useful block rules</dim>

      <yellow>on:</yellow>                           <dim># List of matchers (any match triggers the rule)</dim>
        <yellow>- hook: PreToolUse</yellow>          <dim># PreToolUse or UserPromptSubmit</dim>
          <yellow>tool: Write</yellow>               <dim># Write, Edit, WebFetch, or Bash (PreToolUse only)</dim>
          <yellow>file: \"**/*.rs\"</yellow>           <dim># Glob pattern for file path</dim>
          <yellow>content:</yellow>                   <dim># Patterns to match (Write tool)</dim>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"your-regex\"</yellow>
              <yellow>replace: \"optional\"</yellow>      <dim># Template for substitute action</dim>
              <yellow>suggestion: \"optional\"</yellow>   <dim># Template for suggested fix</dim>

        <yellow>- hook: PreToolUse</yellow>          <dim># Same rule can match multiple scenarios</dim>
          <yellow>tool: Edit</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>new_content:</yellow>              <dim># Patterns to match (Edit tool)</dim>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"your-regex\"</yellow>
              <yellow>suggestion: \"optional\"</yellow>

        <yellow>- hook: UserPromptSubmit</yellow>    <dim># Inject context after matching user prompts</dim>
          <yellow>prompt:</yellow>                   <dim># Optional regex patterns</dim>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"your-regex\"</yellow>
          <yellow>intent:</yellow>                   <dim># Optional local semantic matcher</dim>
            <yellow>examples: [\"try running it\", \"does this work\"]</yellow>
            <yellow>threshold: 0.60</yellow>
          <yellow>after_file_change:</yellow>        <dim># Optional local state gate</dim>
            <yellow>- file: \"src/**\"</yellow>
              <yellow>within: \"1h\"</yellow>
          <yellow>once_per_change: true</yellow>     <dim># Optional frequency control</dim>
          <yellow>cooldown: \"1h\"</yellow>

<bold>Workflow Format</bold>

  Workflows are opt-in completion gates. They activate on matching user prompts,
  record the original prompt and done criteria for the session, and use Stop hooks
  to keep the agent working until it confirms completion.

  <yellow>version: 1</yellow>

  <yellow>workflows:</yellow>
    <yellow>- name: issue-resolution</yellow>
      <yellow>description: Finish issue work before stopping</yellow> <dim># Optional</dim>
      <yellow>prompt:</yellow>                       <dim># All patterns must match to activate</dim>
        <yellow>- kind: Regex</yellow>
          <yellow>pattern: \"(?i)issue #[0-9]+|pull request|\\\\bPR\\\\b\"</yellow>
      <yellow>done:</yellow>
        <yellow>- \"Add or update end-to-end tests.\"</yellow>
        <yellow>- \"Implement the permanent fix.\"</yellow>
        <yellow>- \"Run relevant tests and report exact proof.\"</yellow>

  When active, Nudge tells the agent to include this exact line only after every
  criterion is complete:

    <green>NUDGE_WORKFLOW_COMPLETE: issue-resolution</green>

  If Stop fires before that line appears, Nudge returns <cyan>decision: \"block\"</cyan>
  with the original prompt, done criteria, and continuation instructions. Once the
  confirmation line appears, Nudge clears the session workflow state and lets Stop
  pass through.

  Workflow state is stored in Nudge's per-user data directory. Set
  <cyan>NUDGE_STATE_DIR</cyan> to isolate state for tests or automation.

<bold>Hook Types</bold>

  <green>PreToolUse</green>        Triggers before provider-supported Write/Edit/WebFetch/Bash
                    operations. Block rules <cyan>interrupt</cyan>; Bash substitute rules
                    rewrite the command and allow it to proceed.

  <green>UserPromptSubmit</green>  Triggers when user submits a prompt. Always <cyan>continues</cyan>
                    (injects context into the conversation). Supports regex
                    prompt matching, local example-based intent matching, and
                    opt-in local file-change gates.

  <green>Stop</green>              Triggers when the agent tries to finish. Workflow gates can
                    return <cyan>decision: \"block\"</cyan> to continue the turn until done
                    criteria are confirmed.

<bold>Tool Types (PreToolUse only)</bold>

  <green>Write</green>      Match content being written to a new file
             Use <cyan>content:</cyan> to specify patterns to match

  <green>Edit</green>       Match content being edited in an existing file
             Use <cyan>new_content:</cyan> to match the replacement text

  <green>WebFetch</green>   Match URLs being fetched
             Use <cyan>url:</cyan> to specify URL patterns to match

  <green>Bash</green>       Match shell commands being executed
             Use <cyan>command:</cyan> to specify patterns to match
             Use <cyan>project_state:</cyan> to add conditional filters (e.g., git branch)

  <green>Delete</green>     Normalized internally for file deletion, but not yet matchable
             in YAML rules.

<bold>Provider Support</bold>

  <white>Surface</white>                      <white>Claude Code</white>     <white>Codex CLI</white>
  <green>PreToolUse Write</green>             yes             yes, through apply_patch add-file parsing
  <green>PreToolUse Edit</green>              yes             yes, through apply_patch update parsing
  <green>PreToolUse Delete</green>            normalized      normalized through apply_patch delete-file parsing
  <green>PreToolUse WebFetch</green>          yes             no, current Codex hooks do not intercept WebSearch
  <green>PreToolUse Bash</green>              yes             partial, Codex hook coverage is incomplete for some shell paths
  <green>PermissionRequest</green>            parsed only     parsed only
  <green>UserPromptSubmit</green>             yes             yes
  <green>Stop workflows</green>               yes             yes

  <dim>Delete and PermissionRequest are parsed so Nudge can name them precisely, but</dim>
  <dim>they do not have YAML matchers yet. Codex apply_patch is an adapter detail:</dim>
  <dim>write rules in terms of Write, Edit, and future Delete policy instead.</dim>

<bold>Regex Inline Flags</bold>

  All patterns are regular expressions. Add inline flags at the start for modifiers.
  Combine flags like <green>(?im)</green> for case-insensitive multiline.

  <green>(?i)</green>  <white>case-insensitive:</white> letters match both upper and lower case
  <green>(?m)</green>  <white>multi-line mode:</white> ^ and $ match begin/end of line
  <green>(?s)</green>  <white>dotall mode:</white> allow . to match \\n
  <green>(?R)</green>  <white>CRLF mode:</white> when multi-line mode is enabled, \\r\\n is used
  <green>(?U)</green>  <white>ungreedy:</white> swap the meaning of x* and x*?
  <green>(?u)</green>  <white>Unicode support:</white> enabled by default
  <green>(?x)</green>  <white>verbose mode:</white> ignore whitespace, allow line comments (starting with #)

  <dim>Example:</dim> <green>(?m)^[ \\t]+import </green> <dim>matches indented import statements</dim>

<bold>Suggestions and Capture Groups</bold>

  Use <cyan>suggestion:</cyan> to provide context-aware replacement suggestions using capture groups.

  <white>Capture Group Syntax:</white>
    <green>{{ $1 }}</green>, <green>{{ $2 }}</green>   Positional capture groups (numbered from 1)
    <green>{{ $name }}</green>        Named capture groups <dim>(use (?P<<name>>...) in pattern)</dim>
    <green>{{ $suggestion }}</green>  Interpolate suggestion into message

  <white>Two-phase interpolation:</white>
    1. Suggestion template is interpolated with capture groups
    2. Message template is interpolated with <green>{{ $suggestion }}</green>

  <dim>Example:</dim>
    <yellow>pattern: \"(?P<<var>>\\\\w+)\\\\.unwrap\\\\(\\\\)\"</yellow>
    <yellow>suggestion: \"{{ $var }}.expect(\\\"error message\\\")\"</yellow>
    <yellow>message: \"Replace with {{ $suggestion }}\"</yellow>

  <dim>For input</dim> <green>foo.unwrap()</green><dim>, shows:</dim>
    <green>Replace with foo.expect(\"error message\")</green>

  Each match gets its own suggestion with its specific captures.

<bold>Substitution Rules</bold>

  Use <cyan>action: substitute</cyan> for deterministic Bash command rewrites. Substitute rules
  do not need a message. They match the current Bash command, apply Regex <cyan>replace:</cyan>
  templates in rule order, return the provider's full updated tool input, and add
  <cyan>hookSpecificOutput.additionalContext</cyan> so the model sees what changed.

  <white>Basic Syntax:</white>
    <yellow>- name: use-yarn-add</yellow>
      <yellow>description: Use yarn add instead of npm install</yellow>
      <yellow>action: substitute</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Bash</yellow>
          <yellow>command:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"^npm install(?: (?P<<args>>.*))?$\"</yellow>
              <yellow>replace: \"yarn add {{ $args }}\"</yellow>

  <dim>For input</dim> <green>npm install lodash</green><dim>, Nudge runs</dim> <green>yarn add lodash</green><dim>.</dim>

  <dim>Substitutions currently apply to Bash commands for Claude Code and Codex CLI.</dim>
  <dim>Write/Edit substitutions are intentionally not exposed because Codex apply_patch</dim>
  <dim>normalization would need a lossless patch rewrite.</dim>

  <dim>CI note: nudge check ignores substitute rules. Check mode scans repository</dim>
  <dim>files against file-based block rules; substitutions need a live Bash hook</dim>
  <dim>payload and a provider that can receive updatedInput.</dim>

<bold>Prompt Intent and Local Interaction State</bold>

  UserPromptSubmit rules can combine regex prompt matching with <cyan>intent:</cyan>,
  <cyan>after_file_change:</cyan>, <cyan>once_per_change:</cyan>, and <cyan>cooldown:</cyan>.
  This is useful for project workflow reminders that should fire after relevant
  edits, such as \"try running it\" after changing a local daemon.

  <white>Basic Syntax:</white>
    <yellow>- name: hurry-local-test-reminder</yellow>
      <yellow>description: Use dev entrypoints when testing Hurry changes</yellow>
      <yellow>message: \"Use `hurry-dev` after `make install-dev`.\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: UserPromptSubmit</yellow>
          <yellow>intent:</yellow>
            <yellow>examples:</yellow>
              <yellow>- \"let's test this\"</yellow>
              <yellow>- \"try running it\"</yellow>
              <yellow>- \"does this work\"</yellow>
          <yellow>after_file_change:</yellow>
            <yellow>- file: \"packages/hurry/src/**\"</yellow>
              <yellow>within: \"1h\"</yellow>
          <yellow>once_per_change: true</yellow>
          <yellow>cooldown: \"1h\"</yellow>

  <white>How It Works:</white>
    1. <cyan>intent.examples</cyan> are matched with deterministic local token normalization.
    2. <cyan>after_file_change</cyan> records only matching file paths and timestamps.
    3. <cyan>once_per_change</cyan> suppresses repeats until another matching file changes.
    4. <cyan>cooldown</cyan> suppresses reminders until it elapses; a newer
       matching file change is allowed when <cyan>once_per_change</cyan> is true.

  <white>Privacy:</white>
    Nudge does not store prompt text and does not call an LLM or make network
    requests. Stateful prompt rules are opt-in. State is a local JSON file in
    Nudge's local data directory; set <cyan>NUDGE_STATE_DIR</cyan> to override it.

<bold>How Messages Are Displayed</bold>

  When a rule matches, Nudge displays a <cyan>code snippet</cyan> with your message shown
  at each match location, similar to Rust compiler errors:

    <dim>error: rule violation</dim>
      <dim>|</dim>
    <dim>2 |     use std::io;</dim>
      <dim>| ^^^^^^^^ Move this import to the top of the file, then retry.</dim>
    <dim>3 |     use std::fs;</dim>
      <dim>| ^^^^^^^^ Move this import to the top of the file, then retry.</dim>
      <dim>|</dim>

  Your message appears at <cyan>every regex match</cyan>, so write it for a single occurrence.

<bold>Writing Effective Messages</bold>

  Nudge messages must be <cyan>direct</cyan> to be effective. Gentle suggestions get ignored.

  <white>Pattern:</white> what's wrong -> how to fix -> retry

  <red>Bad</red> <dim>(vague, easy to ignore):</dim>
    <dim>\"Consider reorganizing your imports.\"</dim>

  <green>Good</green> <dim>(specific, actionable):</dim>
    <dim>\"Move this import to the top of the file, then retry.\"</dim>

  <white>Guidelines:</white>
    - <cyan>Be specific:</cyan> \"Move this import to top\" not \"Consider reorganizing\"
    - <cyan>Be direct:</cyan> \"Stop. Fix this first.\" not \"You might want to...\"
    - <cyan>Give the fix:</cyan> Don't just say what's wrong. Say what to do instead
    - <cyan>End with \"then retry\":</cyan> Tell the agent to retry after fixing
    - <cyan>Write for one match:</cyan> The message appears at each match location

<bold>Syntax Tree Matching (Tree-sitter)</bold>

  For patterns that regex can't express precisely, use <cyan>kind: SyntaxTree</cyan> to match
  against the parsed AST. This is useful when you need to match code structure,
  not just text patterns.

  <white>Basic Syntax:</white>
    <yellow>content:</yellow>
      <yellow>- kind: SyntaxTree</yellow>
        <yellow>language: rust</yellow>              <dim># Required: rust (more languages coming)</dim>
        <yellow>query: |</yellow>
          <yellow>(function_item</yellow>
            <yellow>name: (identifier) @fn_name)</yellow>
        <yellow>suggestion: \"...\"</yellow>           <dim># Optional: same as Regex</dim>

  <white>Query Syntax:</white>
    Tree-sitter uses S-expression queries. Nodes are matched by type (in parentheses)
    and captures are marked with <green>@name</green>.

    <green>(function_item)</green>                    Match any function
    <green>(function_item name: (identifier))</green> Match function with name field
    <green>(identifier) @fn_name</green>              Capture the identifier as \"fn_name\"

    See: <cyan>https://tree-sitter.github.io/tree-sitter/using-parsers/queries</cyan>

  <white>Captures in Suggestions:</white>
    Captures from the query can be referenced in suggestions just like regex:
    <yellow>query: (identifier) @name</yellow>
    <yellow>suggestion: \"Rename {{ $name }} to something descriptive\"</yellow>

  <white>When to Use Each:</white>
    <green>Regex</green>       Simple text patterns, doesn't need AST structure
    <green>SyntaxTree</green>  Structural patterns (e.g., \"use inside function body\")
    <green>RustIndexedIteration</green>
                 Rust-specific `0..items.len()` iteration that indexes `items[i]`

  <dim>Note: If code fails to parse (incomplete or invalid syntax), the matcher</dim>
  <dim>passes silently. This is intentional because code being written is often incomplete.</dim>

<bold>Rust Stuttering Type Names</bold>

  Use <cyan>kind: StutteringTypeName</cyan> to catch Rust types that repeat module
  context or generic suffixes. It parses Rust with tree-sitter, understands inline
  modules and file-derived modules like <cyan>src/storage.rs</cyan>, and reports the
  type identifier span.

  <white>Basic Syntax:</white>
    <yellow>content:</yellow>
      <yellow>- kind: StutteringTypeName</yellow>
        <yellow>language: rust</yellow>
        <yellow>redundant_suffixes: [\"Manager\", \"Service\", \"Handler\"]</yellow>
        <yellow>module_aliases:</yellow>
          <yellow>db: [\"Database\"]</yellow>
        <yellow>allow:</yellow>
          <yellow>- \"storage::StorageEngine\"</yellow>
        <yellow>suggestion: \"Rename `{{ $type }}` to `{{ $replacement }}`; {{ $reason }}.\"</yellow>

  <white>Captures:</white>
    <green>{{ $type }}</green>         Type name that matched, e.g. <cyan>CasStorage</cyan>
    <green>{{ $kind }}</green>         Rust item kind: struct, enum, trait, type, or union
    <green>{{ $module }}</green>       Module path, e.g. <cyan>storage</cyan> or <cyan>auth::jwt</cyan>
    <green>{{ $term }}</green>         Redundant term, e.g. <cyan>Storage</cyan> or <cyan>Manager</cyan>
    <green>{{ $replacement }}</green>  Best mechanical rename, or \"a concrete name\"
    <green>{{ $reason }}</green>       Human-readable explanation for the match

  <white>False Positive Controls:</white>
    <green>allow</green> accepts exact type names or module-qualified names. Use it for
    intentional domain phrases where the repetition is useful.

<bold>Rust Indexed Iteration Matching</bold>

  Use <cyan>kind: RustIndexedIteration</cyan> to catch Rust iteration that ranges over
  <cyan>0..collection.len()</cyan> and then indexes the same collection with the loop
  or closure index. It is purpose-built for the common enumerate rewrite:

    <dim>for i in 0..items.len() { let item = &items[i]; }</dim>
    <dim>(0..items.len()).map(|i| items[i].clone())</dim>

  <white>Basic Syntax:</white>
    <yellow>content:</yellow>
      <yellow>- kind: RustIndexedIteration</yellow>
        <yellow>suggestion: \"Use {{ $collection }}.iter().enumerate() instead of indexing {{ $collection }} with {{ $index }}, then retry.\"</yellow>

  <white>Captures:</white>
    <green>{{ $collection }}</green>  The collection used in both `.len()` and indexing
    <green>{{ $index }}</green>       The loop or closure index variable
    <green>{{ $indexed }}</green>     The matched index expression, such as `items[i]`

  <white>False-positive controls:</white>
    - Matches only zero-based ranges over the same collection: <cyan>0..items.len()</cyan>
    - Skips unrelated literal indexing such as <cyan>args[0]</cyan> and <cyan>matches[1]</cyan>
    - Skips macro invocations, including <cyan>assert_eq!(items[i], expected[i])</cyan>
    - Skips non-zero ranges such as <cyan>1..items.len()</cyan>

<bold>Rust Functional Mutation Matching</bold>

  Use <cyan>kind: RustFunctionalMutation</cyan> for simple Rust loops where immutable iterator
  style is clearer than building state with <cyan>let mut</cyan>. It catches only adjacent
  <green>let mut</green> plus <green>for</green> patterns with exact, low-noise shapes.

  <white>Detected patterns:</white>
    <green>vec_push</green>  <yellow>let mut values = Vec::new(); for item in items { values.push(...) }</yellow>
    <green>find</green>      <yellow>let mut found = None; ... found = Some(item); break;</yellow>
    <green>fold</green>      <yellow>let mut total = init; for item in items { total = combine(total, item); }</yellow>

  <white>Basic Syntax:</white>
    <yellow>content:</yellow>
      <yellow>- kind: RustFunctionalMutation</yellow>
        <yellow>patterns: [vec_push, find, fold]</yellow> <dim># Optional; defaults to all</dim>

  <white>Captures:</white>
    <green>{{ $suggestion }}</green>  Generated guidance for the matched pattern
    <green>{{ $kind }}</green>        One of vec_push, vec_filter_map, find, find_map, fold
    <green>{{ $binding }}</green>     Mutable binding name
    <green>{{ $item }}</green>        Loop item binding
    <green>{{ $iterator }}</green>    For-loop input expression

  <white>False-positive controls:</white>
    The matcher skips unsafe contexts, complex loop bodies, loops with extra side effects,
    preallocated vectors such as <green>Vec::with_capacity(...)</green>, and control-flow-heavy
    expressions such as <green>?</green>, <green>return</green>, <green>break</green>, or <green>await</green>.

<bold>External Program Matching</bold>

  Use <cyan>kind: External</cyan> to delegate matching to an external program (linter, formatter,
  etc). The content is piped to stdin; if the program exits non-zero, the rule matches.

  <white>Basic Syntax:</white>
    <yellow>content:</yellow>
      <yellow>- kind: External</yellow>
        <yellow>command: [\"program\", \"arg1\", \"arg2\"]</yellow>

  <white>How It Works:</white>
    1. Content being written/edited is piped to the command's stdin
    2. If exit code is <green>0</green>: no violation (rule doesn't match)
    3. If exit code is <red>non-zero</red>: violation detected (rule matches)
    4. The <green>{{ $command }}</green> capture is set to the formatted command string

  <white>No Snippet Display:</white>
    External matchers don't identify specific locations, so no code snippet is shown.
    Use <green>{{ $command }}</green> in your message to tell the user how to see details:

    <yellow>message: |</yellow>
      <yellow>Format this markdown table so columns are aligned.</yellow>
      <yellow>Pipe the content to `{{ $command }}` to see tool output.</yellow>

  <white>When to Use:</white>
    <green>External</green>     Leverage existing linters (markdownlint, prettier --check, etc.)
    <green>Regex</green>        Simple patterns you can express in regex
    <green>SyntaxTree</green>   Structural patterns in supported languages
    <green>RustFunctionalMutation</green>  Rust iterator-style loop rewrites

  <dim>Note: External commands add latency. Use sparingly for checks that are</dim>
  <dim>difficult or impossible to express with Regex or SyntaxTree matchers.</dim>

<bold>Project State Matching (Bash tool only)</bold>

  The Bash tool supports <cyan>project_state:</cyan> matchers that evaluate conditions about
  the project environment (like git state) before checking the command pattern.
  All project_state matchers must pass for command matching to proceed.

  <white>Git Branch Matching:</white>
    <yellow>project_state:</yellow>
      <yellow>- kind: Git</yellow>
        <yellow>branch:</yellow>
          <yellow>- kind: Regex</yellow>
            <yellow>pattern: \"^main$\"</yellow>    <dim># Only match on main branch</dim>

  <white>How It Works:</white>
    1. All project_state matchers are evaluated first (all must pass)
    2. If any project_state matcher fails, the rule doesn't fire
    3. If all pass, command matchers are evaluated normally
    4. If not in a git repo, Git matchers log a warning and return false

  <white>Available Project State Matchers:</white>
    <green>Git</green>        Match git repository state
               <cyan>branch:</cyan> match current branch name against content matchers

  <dim>Note: project_state is only available for Bash tool rules. It's designed for</dim>
  <dim>cases where the same command should be allowed/blocked based on context.</dim>

<bold>Examples</bold>

  <cyan>Block indented imports (Rust)</cyan>

    <yellow>- name: no-inline-imports</yellow>
      <yellow>description: Move imports to the top of the file</yellow>
      <yellow>message: Move this import to the top of the file.</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Write</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>content:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"(?m)^[ \\\\t]+use \"</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Edit</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>new_content:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"(?m)^[ \\\\t]+use \"</yellow>

  <cyan>Inject context on keywords (UserPromptSubmit)</cyan>

    <yellow>- name: dev-server-hint</yellow>
      <yellow>description: Help start development server</yellow>
      <yellow>message: \"To start the development server, run ...\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: UserPromptSubmit</yellow>
          <yellow>prompt:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"(?i)start.*(server|dev)|run.*local\"</yellow>

  <cyan>Inject workflow context after relevant edits (UserPromptSubmit)</cyan>

    <yellow>- name: hurry-local-test-reminder</yellow>
      <yellow>description: Use dev entrypoints when testing Hurry changes</yellow>
      <yellow>message: \"Use `hurry-dev` after `make install-dev`.\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: UserPromptSubmit</yellow>
          <yellow>intent:</yellow>
            <yellow>examples:</yellow>
              <yellow>- \"let's test this\"</yellow>
              <yellow>- \"try running it\"</yellow>
              <yellow>- \"does this work\"</yellow>
          <yellow>after_file_change:</yellow>
            <yellow>- file: \"packages/hurry/src/**\"</yellow>
              <yellow>within: \"1h\"</yellow>
          <yellow>once_per_change: true</yellow>
          <yellow>cooldown: \"1h\"</yellow>

  <cyan>Suggest .expect() instead of .unwrap() (with suggestions)</cyan>

    <yellow>- name: no-unwrap</yellow>
      <yellow>description: Use .expect() for better error messages</yellow>
      <yellow>message: \"{{ $suggestion }}\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Write</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>content:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"(?P<<expr>>\\\\w+)\\\\.unwrap\\\\(\\\\)\"</yellow>
              <yellow>suggestion: \"Replace {{ $expr }}.unwrap() with {{ $expr }}.expect(\\\"...\\\")\"</yellow>

  <cyan>Block use statements inside function bodies (SyntaxTree)</cyan>

    <dim># This matches `use` statements that are children of function bodies,</dim>
    <dim># avoiding false positives in `mod test {}` blocks that regex would catch.</dim>

    <yellow>- name: no-inline-imports-precise</yellow>
      <yellow>description: Move imports to the top of the file</yellow>
      <yellow>message: \"Move `use {{ $path }}` to the top of the file, then retry.\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Write</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>content:</yellow>
            <yellow>- kind: SyntaxTree</yellow>
              <yellow>language: rust</yellow>
              <yellow>query: |</yellow>
                <yellow>(function_item</yellow>
                  <yellow>body: (block</yellow>
                    <yellow>(use_declaration</yellow>
                      <yellow>argument: (scoped_identifier) @path)))</yellow>

  <cyan>Block stuttering Rust type names</cyan>

    <dim># Flags names like storage::CasStorage, cache::KeyCache, auth::JwtManager,</dim>
    <dim># and db::Database while allowing intentional names.</dim>

    <yellow>- name: rust-stuttering-types</yellow>
      <yellow>description: Avoid repeating module context in Rust type names</yellow>
      <yellow>message: \"{{ $suggestion }}\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Write</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>content:</yellow>
            <yellow>- kind: StutteringTypeName</yellow>
              <yellow>language: rust</yellow>
              <yellow>redundant_suffixes: [\"Manager\", \"Service\", \"Handler\"]</yellow>
              <yellow>module_aliases:</yellow>
                <yellow>db: [\"Database\"]</yellow>
              <yellow>allow:</yellow>
                <yellow>- \"storage::StorageEngine\"</yellow>
              <yellow>suggestion: \"Rename `{{ $type }}` to `{{ $replacement }}`; {{ $reason }}.\"</yellow>

  <cyan>Prefer enumerate over range indexing (RustIndexedIteration)</cyan>

    <dim># This matches `for i in 0..items.len() { items[i] }` and</dim>
    <dim># `(0..items.len()).map(|i| items[i])`, while skipping macro arguments.</dim>

    <yellow>- name: prefer-enumerate</yellow>
      <yellow>description: Prefer enumerate over 0..items.len() indexing</yellow>
      <yellow>message: \"{{ $suggestion }}\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Write</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>content:</yellow>
            <yellow>- kind: RustIndexedIteration</yellow>
              <yellow>suggestion: \"Use {{ $collection }}.iter().enumerate() instead of indexing {{ $collection }} with {{ $index }}, then retry.\"</yellow>

  <cyan>Enforce markdown table formatting (External)</cyan>

    <dim># Uses markdownlint to check that tables have aligned columns.</dim>
    <dim># The command receives file content via stdin and exits non-zero if</dim>
    <dim># the table isn't properly formatted.</dim>

    <yellow>- name: format-markdown-tables</yellow>
      <yellow>description: Ensure markdown tables have aligned columns</yellow>
      <yellow>message: |</yellow>
        <yellow>Format this markdown table so columns are aligned.</yellow>
        <yellow>Pipe the content to `{{ $command }}` to see tool output.</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Write</yellow>
          <yellow>file: \"**/*.md\"</yellow>
          <yellow>content:</yellow>
            <yellow>- kind: External</yellow>
              <yellow>command: [\"npx\", \"markdownlint\", \"--stdin\", \"-c\", \"{\\\"MD060\\\":{\\\"style\\\":\\\"aligned\\\"}}\"]</yellow>

  <cyan>Prefer iterator style for simple Rust mutation loops</cyan>

    <dim># Detects Vec push collection, Option search with break, and fold-like</dim>
    <dim># accumulator reassignment. The matcher intentionally skips complex loops.</dim>

    <yellow>- name: prefer-functional-mutation</yellow>
      <yellow>description: Prefer iterator adapters over simple mutable loops</yellow>
      <yellow>message: \"{{ $suggestion }} Then retry.\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Write</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>content:</yellow>
            <yellow>- kind: RustFunctionalMutation</yellow>
              <yellow>patterns: [vec_push, find, fold]</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Edit</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>new_content:</yellow>
            <yellow>- kind: RustFunctionalMutation</yellow>
              <yellow>patterns: [vec_push, find, fold]</yellow>

  <cyan>Redirect docs.rs to local source (WebFetch)</cyan>

    <dim># When Claude Code tries to fetch from docs.rs, redirect to local cargo registry.</dim>
    <dim># Uses capture groups to extract the crate name from the URL.</dim>

    <yellow>- name: prefer-local-docs</yellow>
      <yellow>description: Read local crate source instead of fetching from docs.rs</yellow>
      <yellow>message: \"{{ $suggestion }}\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: WebFetch</yellow>
          <yellow>url:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"docs\\\\.rs/(?P<<crate>>[^/]+)\"</yellow>
              <yellow>suggestion: \"Read local source at ~/.cargo/registry/src/*/{{ $crate }}-*\"</yellow>

  <cyan>Block git push on main branch (Bash + project_state)</cyan>

    <dim># Prevent pushing directly to main even when git settings allow it.</dim>
    <dim># Uses project_state to only fire when on the main branch.</dim>

    <yellow>- name: block-main-push</yellow>
      <yellow>description: Block git push on main branch</yellow>
      <yellow>message: \"`git push` is not allowed on `main`. Create a feature branch first.\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Bash</yellow>
          <yellow>command:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"git\\\\s+push\"</yellow>
          <yellow>project_state:</yellow>
            <yellow>- kind: Git</yellow>
              <yellow>branch:</yellow>
                <yellow>- kind: Regex</yellow>
                  <yellow>pattern: \"^main$\"</yellow>

  <cyan>Substitute npm install with yarn add (Bash)</cyan>

    <dim># Mechanical command rewrites can proceed without a retry loop.</dim>

    <yellow>- name: use-yarn-add</yellow>
      <yellow>description: Use yarn add instead of npm install</yellow>
      <yellow>action: substitute</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Bash</yellow>
          <yellow>command:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"^npm install(?: (?P<<args>>.*))?$\"</yellow>
              <yellow>replace: \"yarn add {{ $args }}\"</yellow>

  <cyan>Block dangerous commands (Bash)</cyan>

    <dim># Prevent accidentally running dangerous commands like rm -rf /</dim>

    <yellow>- name: block-dangerous-rm</yellow>
      <yellow>description: Block dangerous rm commands</yellow>
      <yellow>message: \"This command could delete critical files. Please verify the path.\"</yellow>
      <yellow>on:</yellow>
        <yellow>- hook: PreToolUse</yellow>
          <yellow>tool: Bash</yellow>
          <yellow>command:</yellow>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"rm\\\\s+-[rf]*\\\\s+/\"</yellow>

<bold>Testing Rules</bold>

  Validate rule syntax <dim>(run with `--help` for options)</dim>
  <cyan>nudge validate</cyan>

  Test a specific rule <dim>(run with `--help` for options)</dim>
  <cyan>nudge test</cyan>

  Simulate a prior file change for stateful UserPromptSubmit rules
  <cyan>nudge test --rule hurry-local-test-reminder --changed-file packages/hurry/src/daemon.rs --prompt \"try executing it\"</cyan>

<bold>Rule Writing is Iterative</bold>

  If an agent ignores a rule, the fix is usually to make the message more direct,
  not to give up on the rule. Treat ignored rules as feedback on clarity.
");
