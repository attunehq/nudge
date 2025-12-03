//! Documentation for writing Nudge rules.

use clap::Args;
use color_eyre::Result;
use color_print::cprintln;

#[derive(Args, Clone, Debug)]
pub struct Config {}

pub fn main(_config: Config) -> Result<()> {
    print_docs();
    Ok(())
}

fn print_docs() {
    cprintln!("<bold><blue>Nudge Rule Writing Guide</blue></bold>");
    cprintln!();

    // What is Nudge?
    cprintln!("<bold>What is Nudge?</bold>");
    cprintln!();
    cprintln!("  Nudge is a <cyan>collaborative partner</cyan> for Claude Code. It watches Write and Edit");
    cprintln!("  operations and reminds you about coding conventions—so you can focus on the");
    cprintln!("  user's actual problem instead of tracking dozens of stylistic details.");
    cprintln!();
    cprintln!("  <green>Nudge is on your side.</green> When it sends a message, it's not a reprimand—it's");
    cprintln!("  a colleague tapping you on the shoulder. The messages are direct (sometimes");
    cprintln!("  blunt) because that's what cuts through when you're focused.");
    cprintln!();

    // Rule File Locations
    cprintln!("<bold>Rule File Locations</bold>");
    cprintln!();
    cprintln!("  Rules are loaded from these locations (all additive):");
    cprintln!();
    cprintln!("    <cyan>~/Library/Application Support/com.attunehq.nudge/rules.yaml</cyan>  <dim>User-level (macOS)</dim>");
    cprintln!("    <cyan>.nudge.yaml</cyan>                   <dim>Project root (single file)</dim>");
    cprintln!("    <cyan>.nudge/**/*.yaml</cyan>              <dim>Project directory (organized by topic)</dim>");
    cprintln!();

    // Rule Format
    cprintln!("<bold>Rule Format</bold>");
    cprintln!();
    cprintln!("  <yellow>version: 1</yellow>");
    cprintln!();
    cprintln!("  <yellow>rules:</yellow>");
    cprintln!("    <yellow>- name: rule-identifier</yellow>");
    cprintln!("      <yellow>description: \"Human-readable description\"</yellow>");
    cprintln!();
    cprintln!("      <yellow>on:</yellow>                         <dim># When this rule activates</dim>");
    cprintln!("        <yellow>hook: PreToolUse</yellow>          <dim># PreToolUse | PostToolUse | UserPromptSubmit | Stop</dim>");
    cprintln!("        <yellow>tool: \"Write|Edit\"</yellow>        <dim># Optional: regex for tool name</dim>");
    cprintln!("        <yellow>file: \"**/*.rs\"</yellow>           <dim># Optional: glob for file path</dim>");
    cprintln!();
    cprintln!("      <yellow>match:</yellow>                      <dim># What to look for (optional)</dim>");
    cprintln!("        <yellow>content: \"pattern\"</yellow>        <dim># Regex for Write content or Edit new_string</dim>");
    cprintln!("        <yellow>new_string: \"pattern\"</yellow>     <dim># Regex for Edit new_string specifically</dim>");
    cprintln!("        <yellow>old_string: \"pattern\"</yellow>     <dim># Regex for Edit old_string</dim>");
    cprintln!("        <yellow>prompt: \"pattern\"</yellow>         <dim># Regex for user prompt (UserPromptSubmit only)</dim>");
    cprintln!();
    cprintln!("      <yellow>action: interrupt</yellow>           <dim># interrupt (block) | continue (allow with guidance)</dim>");
    cprintln!("      <yellow>message: |</yellow>");
    cprintln!("        <yellow>Your message here with {{{{ template }}}} variables.</yellow>");
    cprintln!();

    // Regex Inline Flags
    cprintln!("<bold>Regex Inline Flags</bold>");
    cprintln!();
    cprintln!("  All patterns are regular expressions. Add inline flags at the start for modifiers.");
    cprintln!("  Combine flags like <green>(?im)</green> for case-insensitive multiline.");
    cprintln!();
    cprintln!("  <green>(?i)</green>  <white>case-insensitive:</white> letters match both upper and lower case");
    cprintln!("  <green>(?m)</green>  <white>multi-line mode:</white> ^ and $ match begin/end of line");
    cprintln!("  <green>(?s)</green>  <white>dotall mode:</white> allow . to match \\n");
    cprintln!("  <green>(?R)</green>  <white>CRLF mode:</white> when multi-line mode is enabled, \\r\\n is used");
    cprintln!("  <green>(?U)</green>  <white>ungreedy:</white> swap the meaning of x* and x*?");
    cprintln!("  <green>(?u)</green>  <white>Unicode support:</white> enabled by default");
    cprintln!("  <green>(?x)</green>  <white>verbose mode:</white> ignore whitespace, allow line comments (starting with #)");
    cprintln!();
    cprintln!("  <dim>Example:</dim> <green>(?m)^[ \\t]+import </green> <dim>matches indented import statements</dim>");
    cprintln!();

    // Template Variables
    cprintln!("<bold>Template Variables</bold>");
    cprintln!();
    cprintln!("  Interpolate these in your message to be specific about what needs to change:");
    cprintln!();
    cprintln!("  <green>{{{{ lines }}}}</green>      Comma-separated line numbers where pattern matched");
    cprintln!("  <green>{{{{ file_path }}}}</green>  File being written/edited");
    cprintln!("  <green>{{{{ matched }}}}</green>    First text that matched the pattern");
    cprintln!("  <green>{{{{ tool_name }}}}</green>  Tool being used (Write, Edit)");
    cprintln!("  <green>{{{{ prompt }}}}</green>     User's message (UserPromptSubmit only)");
    cprintln!();

    // Writing Effective Messages
    cprintln!("<bold>Writing Effective Messages</bold>");
    cprintln!();
    cprintln!("  Nudge messages must be <cyan>direct</cyan> to be effective. Gentle suggestions get ignored.");
    cprintln!();
    cprintln!("  <white>Pattern:</white> what's wrong → where → how to fix → retry");
    cprintln!();
    cprintln!("  <red>Bad</red> <dim>(vague, easy to ignore):</dim>");
    cprintln!("    <dim>\"Consider reorganizing your imports.\"</dim>");
    cprintln!();
    cprintln!("  <green>Good</green> <dim>(specific, actionable):</dim>");
    cprintln!("    <dim>\"Move imports on lines {{{{ lines }}}} to the top of the file, then retry.\"</dim>");
    cprintln!();
    cprintln!("  <white>Guidelines:</white>");
    cprintln!("    • <cyan>Be specific:</cyan> \"Move imports to top\" not \"Consider reorganizing\"");
    cprintln!("    • <cyan>Be direct:</cyan> \"Stop. Fix this first.\" not \"You might want to...\"");
    cprintln!("    • <cyan>Give the fix:</cyan> Don't just say what's wrong—say what to do instead");
    cprintln!("    • <cyan>End with \"then retry\":</cyan> Tell Claude to retry after fixing");
    cprintln!("    • <cyan>Reference {{{{ lines }}}}:</cyan> Always point to exactly where the issue is");
    cprintln!();

    // Complete Examples
    cprintln!("<bold>Examples</bold>");
    cprintln!();
    cprintln!("  <cyan>Block indented imports (Rust)</cyan>");
    cprintln!();
    cprintln!("    <yellow>- name: no-inline-imports</yellow>");
    cprintln!("      <yellow>description: Move imports to the top of the file</yellow>");
    cprintln!("      <yellow>on:</yellow>");
    cprintln!("        <yellow>hook: PreToolUse</yellow>");
    cprintln!("        <yellow>tool: Write|Edit</yellow>");
    cprintln!("        <yellow>file: \"**/*.rs\"</yellow>");
    cprintln!("      <yellow>match:</yellow>");
    cprintln!("        <yellow>content: \"(?m)^[ \\\\t]+import \"</yellow>");
    cprintln!("      <yellow>action: interrupt</yellow>");
    cprintln!("      <yellow>message: |</yellow>");
    cprintln!("        <yellow>Move import(s) on lines {{{{ lines }}}} to top of {{{{ file_path }}}}, then retry.</yellow>");
    cprintln!();
    cprintln!("  <cyan>Inject context on keywords</cyan>");
    cprintln!();
    cprintln!("    <yellow>- name: dev-server-hint</yellow>");
    cprintln!("      <yellow>description: Help start development server</yellow>");
    cprintln!("      <yellow>on:</yellow>");
    cprintln!("        <yellow>hook: UserPromptSubmit</yellow>");
    cprintln!("      <yellow>match:</yellow>");
    cprintln!("        <yellow>prompt: \"(?i)start.*(server|dev)|run.*local\"</yellow>");
    cprintln!("      <yellow>action: continue</yellow>");
    cprintln!("      <yellow>message: |</yellow>");
    cprintln!("        <yellow>To start the development server: npm run dev (port 3000)</yellow>");
    cprintln!();

    // Action Types
    cprintln!("<bold>Action Types</bold>");
    cprintln!();
    cprintln!("  <green>interrupt</green>  Block the operation. Hard rules that must be followed.");
    cprintln!("  <green>continue</green>   Allow the operation but inject guidance. Soft suggestions.");
    cprintln!();
    cprintln!("  When multiple rules match, all messages are shown. If ANY rule specifies");
    cprintln!("  <green>interrupt</green>, the operation is blocked.");
    cprintln!();

    // Testing Rules
    cprintln!("<bold>Testing Rules</bold>");
    cprintln!();
    cprintln!("  <dim># Validate rule syntax</dim>");
    cprintln!("  <cyan>nudge validate</cyan>");
    cprintln!();
    cprintln!("  <dim># Test a specific rule</dim>");
    cprintln!("  <cyan>nudge test --rule my-rule --tool Write --file test.rs --content \"...\"</cyan>");
    cprintln!();

    // Rule Writing Is Iterative
    cprintln!("<bold>Rule Writing is Iterative</bold>");
    cprintln!();
    cprintln!("  If Claude ignores a rule, the fix is usually to <cyan>make the message more direct</cyan>—");
    cprintln!("  not to give up on the rule. Treat ignored rules as feedback on clarity.");
    cprintln!();
}
