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
<bold><blue>Nudge Rule Writing Guide</blue></bold>

<bold>What is Nudge?</bold>

  Nudge is a <cyan>collaborative partner</cyan> for Claude Code. It watches Write, Edit,
  WebFetch, and Bash operations and reminds you about coding conventions—so you can
  focus on the user's actual problem instead of tracking dozens of stylistic details.

  <green>Nudge is on your side.</green> When it sends a message, it's not a reprimand—it's
  a colleague tapping you on the shoulder. The messages are direct (sometimes
  blunt) because that's what cuts through when you're focused.

<bold>Rule File Locations</bold>

  Rules are loaded from these locations (all additive):

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
      <yellow>message: \"Your message shown at each match\"</yellow>

      <yellow>on:</yellow>                           <dim># List of matchers (any match triggers the rule)</dim>
        <yellow>- hook: PreToolUse</yellow>          <dim># PreToolUse or UserPromptSubmit</dim>
          <yellow>tool: Write</yellow>               <dim># Write, Edit, or WebFetch (PreToolUse only)</dim>
          <yellow>file: \"**/*.rs\"</yellow>           <dim># Glob pattern for file path</dim>
          <yellow>content:</yellow>                   <dim># Patterns to match (Write tool)</dim>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"your-regex\"</yellow>
              <yellow>suggestion: \"optional\"</yellow>   <dim># Template for suggested fix</dim>

        <yellow>- hook: PreToolUse</yellow>          <dim># Same rule can match multiple scenarios</dim>
          <yellow>tool: Edit</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>new_content:</yellow>              <dim># Patterns to match (Edit tool)</dim>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"your-regex\"</yellow>
              <yellow>suggestion: \"optional\"</yellow>

<bold>Hook Types</bold>

  <green>PreToolUse</green>        Triggers before Write/Edit/WebFetch/Bash operations. Always
                    <cyan>interrupts</cyan> (blocks the operation until the issue is fixed).

  <green>UserPromptSubmit</green>  Triggers when user submits a prompt. Always <cyan>continues</cyan>
                    (injects context into the conversation).

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

<bold>How Messages Are Displayed</bold>

  When a rule matches, Nudge displays a <cyan>code snippet</cyan> with your message shown
  at each match location—similar to Rust compiler errors:

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

  <white>Pattern:</white> what's wrong → how to fix → retry

  <red>Bad</red> <dim>(vague, easy to ignore):</dim>
    <dim>\"Consider reorganizing your imports.\"</dim>

  <green>Good</green> <dim>(specific, actionable):</dim>
    <dim>\"Move this import to the top of the file, then retry.\"</dim>

  <white>Guidelines:</white>
    • <cyan>Be specific:</cyan> \"Move this import to top\" not \"Consider reorganizing\"
    • <cyan>Be direct:</cyan> \"Stop. Fix this first.\" not \"You might want to...\"
    • <cyan>Give the fix:</cyan> Don't just say what's wrong—say what to do instead
    • <cyan>End with \"then retry\":</cyan> Tell Claude to retry after fixing
    • <cyan>Write for one match:</cyan> The message appears at each match location

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

  <dim>Note: If code fails to parse (incomplete or invalid syntax), the matcher</dim>
  <dim>passes silently. This is intentional—code being written is often incomplete.</dim>

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

  <cyan>Redirect docs.rs to local source (WebFetch)</cyan>

    <dim># When Claude tries to fetch from docs.rs, redirect to local cargo registry.</dim>
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

<bold>Rule Writing is Iterative</bold>

  If Claude ignores a rule, the fix is usually to make the message more direct,
  not to give up on the rule. Treat ignored rules as feedback on clarity.
");
