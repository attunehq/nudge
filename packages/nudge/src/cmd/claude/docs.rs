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

  Nudge is a <cyan>collaborative partner</cyan> for Claude Code. It watches Write and Edit
  operations and reminds you about coding conventions—so you can focus on the
  user's actual problem instead of tracking dozens of stylistic details.

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
          <yellow>tool: Write</yellow>               <dim># Write or Edit (PreToolUse only)</dim>
          <yellow>file: \"**/*.rs\"</yellow>           <dim># Glob pattern for file path</dim>
          <yellow>content:</yellow>                   <dim># Patterns to match (Write tool)</dim>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"your-regex\"</yellow>

        <yellow>- hook: PreToolUse</yellow>          <dim># Same rule can match multiple scenarios</dim>
          <yellow>tool: Edit</yellow>
          <yellow>file: \"**/*.rs\"</yellow>
          <yellow>new_content:</yellow>              <dim># Patterns to match (Edit tool)</dim>
            <yellow>- kind: Regex</yellow>
              <yellow>pattern: \"your-regex\"</yellow>

<bold>Hook Types</bold>

  <green>PreToolUse</green>        Triggers before Write/Edit operations. Always <cyan>interrupts</cyan>
                    (blocks the operation until the issue is fixed).

  <green>UserPromptSubmit</green>  Triggers when user submits a prompt. Always <cyan>continues</cyan>
                    (injects context into the conversation).

<bold>Tool Types (PreToolUse only)</bold>

  <green>Write</green>   Match content being written to a new file
          Use <cyan>content:</cyan> to specify patterns to match

  <green>Edit</green>    Match content being edited in an existing file
          Use <cyan>new_content:</cyan> to match the replacement text

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

<bold>Testing Rules</bold>

  Validate rule syntax <dim>(run with `--help` for options)</dim>
  <cyan>nudge validate</cyan>

  Test a specific rule <dim>(run with `--help` for options)</dim>
  <cyan>nudge test</cyan>

<bold>Rule Writing is Iterative</bold>

  If Claude ignores a rule, the fix is usually to make the message more direct,
  not to give up on the rule. Treat ignored rules as feedback on clarity.
");
