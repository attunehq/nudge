//! Rule evaluation and response aggregation.

use color_eyre::{
    Section, SectionExt,
    eyre::{Context, Result},
};
use glob::Pattern;
use regex::Regex;

use crate::claude::hook::{
    Hook, PostToolUsePayload, PreToolUsePayload, StopPayload, UserPromptSubmitPayload,
};

use super::schema::{Action, HookType, Match, Rule};

/// A rule compiled for efficient evaluation.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub name: String,

    hook_type: HookType,

    tool_pattern: Option<Regex>,

    file_glob: Option<Pattern>,

    content_pattern: Option<Regex>,

    new_string_pattern: Option<Regex>,

    old_string_pattern: Option<Regex>,

    prompt_pattern: Option<Regex>,

    message_pattern: Option<Regex>,

    action: Action,

    message_template: String,
}

/// Result of evaluating a single rule.
#[derive(Debug)]
pub struct RuleResult {
    /// The message to display to the user.
    pub message: String,

    /// The action to take when the rule matches.
    pub action: Action,
}

/// Context gathered during matching for template interpolation.
#[derive(Debug, Default)]
struct MatchContext {
    /// The lines that matched the pattern.
    lines: Vec<usize>,

    /// The file path that matched the pattern.
    file_path: Option<String>,

    /// The text that matched the pattern.
    matched: Option<String>,

    /// The tool name that matched the pattern.
    tool_name: Option<String>,

    /// The user prompt that matched the pattern.
    prompt: Option<String>,
}

impl CompiledRule {
    /// Compile a rule from its schema representation.
    #[tracing::instrument(name = "CompiledRule::compile")]
    pub fn compile(rule: Rule) -> Result<Self> {
        let tool_pattern = rule
            .on
            .tool
            .as_ref()
            .map(|p| build_regex(p))
            .transpose()
            .with_context(|| format!("invalid tool regex in rule '{}'", rule.name))
            .with_section(|| format!("{rule:#?}").header("Rule:"))?;

        let file_glob = rule
            .on
            .file
            .as_ref()
            .map(|p| Pattern::new(p))
            .transpose()
            .with_context(|| format!("invalid file glob in rule '{}'", rule.name))
            .with_section(|| format!("{rule:#?}").header("Rule:"))?;

        let Match {
            ref content,
            ref new_string,
            ref old_string,
            ref prompt,
            ref message,
        } = rule.r#match;

        let content_pattern = content
            .as_ref()
            .map(|p| build_regex(p))
            .transpose()
            .with_context(|| format!("invalid content regex in rule '{}'", rule.name))
            .with_section(|| format!("{rule:#?}").header("Rule:"))?;

        let new_string_pattern = new_string
            .as_ref()
            .map(|p| build_regex(p))
            .transpose()
            .with_context(|| format!("invalid new_string regex in rule '{}'", rule.name))
            .with_section(|| format!("{rule:#?}").header("Rule:"))?;

        let old_string_pattern = old_string
            .as_ref()
            .map(|p| build_regex(p))
            .transpose()
            .with_context(|| format!("invalid old_string regex in rule '{}'", rule.name))
            .with_section(|| format!("{rule:#?}").header("Rule:"))?;

        let prompt_pattern = prompt
            .as_ref()
            .map(|p| build_regex(p))
            .transpose()
            .with_context(|| format!("invalid prompt regex in rule '{}'", rule.name))
            .with_section(|| format!("{rule:#?}").header("Rule:"))?;

        let message_pattern = message
            .as_ref()
            .map(|p| build_regex(p))
            .transpose()
            .with_context(|| format!("invalid message regex in rule '{}'", rule.name))
            .with_section(|| format!("{rule:#?}").header("Rule:"))?;

        Ok(CompiledRule {
            name: rule.name,
            hook_type: rule.on.hook,
            tool_pattern,
            file_glob,
            content_pattern,
            new_string_pattern,
            old_string_pattern,
            prompt_pattern,
            message_pattern,
            action: rule.action,
            message_template: rule.message,
        })
    }

    /// Evaluate this rule against a hook. Returns None if the rule doesn't match.
    #[tracing::instrument(name = "CompiledRule::evaluate")]
    pub fn evaluate(&self, hook: &Hook) -> Option<RuleResult> {
        let mut ctx = MatchContext::default();

        if !self.matches_hook_type(hook) {
            return None;
        }

        match hook {
            Hook::PreToolUse(payload) => {
                if !self.matches_pre_tool_use(payload, &mut ctx) {
                    return None;
                }
            }
            Hook::PostToolUse(payload) => {
                if !self.matches_post_tool_use(payload, &mut ctx) {
                    return None;
                }
            }
            Hook::UserPromptSubmit(payload) => {
                if !self.matches_user_prompt(payload, &mut ctx) {
                    return None;
                }
            }
            Hook::Stop(payload) => {
                if !self.matches_stop(payload, &mut ctx) {
                    return None;
                }
            }
        }

        let message = self.render_message(&ctx);
        Some(RuleResult {
            message,
            action: self.action,
        })
    }

    fn matches_hook_type(&self, hook: &Hook) -> bool {
        match (self.hook_type, hook) {
            (HookType::PreToolUse, Hook::PreToolUse(_)) => true,
            (HookType::PostToolUse, Hook::PostToolUse(_)) => true,
            (HookType::UserPromptSubmit, Hook::UserPromptSubmit(_)) => true,
            (HookType::Stop, Hook::Stop(_)) => true,
            _ => false,
        }
    }

    fn matches_pre_tool_use(&self, payload: &PreToolUsePayload, ctx: &mut MatchContext) -> bool {
        ctx.tool_name = Some(payload.tool_name.clone());

        if let Some(ref pattern) = self.tool_pattern {
            if !pattern.is_match(&payload.tool_name) {
                return false;
            }
        }

        let file_path = payload.tool_input.get("file_path").and_then(|v| v.as_str());
        if let Some(fp) = file_path {
            ctx.file_path = Some(fp.to_string());
        }

        if let Some(ref glob) = self.file_glob {
            match file_path {
                Some(fp) => {
                    if !glob.matches(fp) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        match payload.tool_name.as_str() {
            "Write" => {
                let content = payload.tool_input.get("content").and_then(|v| v.as_str());
                if let Some(ref pattern) = self.content_pattern {
                    match content {
                        Some(c) => {
                            if !self.check_content_match(pattern, c, ctx) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
            }
            "Edit" => {
                let new_string = payload
                    .tool_input
                    .get("new_string")
                    .and_then(|v| v.as_str());
                let old_string = payload
                    .tool_input
                    .get("old_string")
                    .and_then(|v| v.as_str());

                // Check new_string pattern (also check content pattern against new_string for convenience)
                if let Some(ref pattern) = self.new_string_pattern {
                    match new_string {
                        Some(s) => {
                            if !self.check_content_match(pattern, s, ctx) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                if let Some(ref pattern) = self.content_pattern {
                    match new_string {
                        Some(s) => {
                            if !self.check_content_match(pattern, s, ctx) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }

                // Check old_string pattern
                if let Some(ref pattern) = self.old_string_pattern {
                    match old_string {
                        Some(s) => {
                            if !pattern.is_match(s) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
            }
            _ => {
                // For other tools, if content pattern is specified but no content available, skip
                if self.content_pattern.is_some() {
                    return false;
                }
            }
        }

        true
    }

    fn matches_post_tool_use(&self, payload: &PostToolUsePayload, ctx: &mut MatchContext) -> bool {
        ctx.tool_name = Some(payload.tool_name.clone());

        // Check tool name pattern
        if let Some(ref pattern) = self.tool_pattern {
            if !pattern.is_match(&payload.tool_name) {
                return false;
            }
        }

        // Extract file_path if available
        let file_path = payload.tool_input.get("file_path").and_then(|v| v.as_str());

        if let Some(fp) = file_path {
            ctx.file_path = Some(fp.to_string());
        }

        // Check file glob
        if let Some(ref glob) = self.file_glob {
            match file_path {
                Some(fp) => {
                    if !glob.matches(fp) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // For PostToolUse, we can check the same content patterns as PreToolUse
        // (the tool_input is the same)
        match payload.tool_name.as_str() {
            "Write" => {
                let content = payload.tool_input.get("content").and_then(|v| v.as_str());
                if let Some(ref pattern) = self.content_pattern {
                    match content {
                        Some(c) => {
                            if !self.check_content_match(pattern, c, ctx) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
            }
            "Edit" => {
                let new_string = payload
                    .tool_input
                    .get("new_string")
                    .and_then(|v| v.as_str());

                if let Some(ref pattern) = self.new_string_pattern {
                    match new_string {
                        Some(s) => {
                            if !self.check_content_match(pattern, s, ctx) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                if let Some(ref pattern) = self.content_pattern {
                    match new_string {
                        Some(s) => {
                            if !self.check_content_match(pattern, s, ctx) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
            }
            _ => {
                if self.content_pattern.is_some() {
                    return false;
                }
            }
        }

        true
    }

    fn matches_user_prompt(
        &self,
        payload: &UserPromptSubmitPayload,
        ctx: &mut MatchContext,
    ) -> bool {
        ctx.prompt = Some(payload.prompt.clone());

        if let Some(ref pattern) = self.prompt_pattern {
            if let Some(m) = pattern.find(&payload.prompt) {
                ctx.matched = Some(m.as_str().to_string());
            } else {
                return false;
            }
        }

        true
    }

    fn matches_stop(&self, _payload: &StopPayload, _ctx: &mut MatchContext) -> bool {
        // StopPayload doesn't have message content to match against currently
        // The schema mentions `message` but StopPayload only has stop_hook_active
        // For now, Stop rules without message pattern will always match
        if self.message_pattern.is_some() {
            // Can't match message pattern without message content
            return false;
        }
        true
    }

    /// Check if content matches pattern and update context with match info.
    fn check_content_match(&self, pattern: &Regex, content: &str, ctx: &mut MatchContext) -> bool {
        if !pattern.is_match(content) {
            return false;
        }

        // Find first match for {{ matched }}
        if let Some(m) = pattern.find(content) {
            ctx.matched = Some(m.as_str().to_string());
        }

        // Find all matching line numbers
        for (i, line) in content.lines().enumerate() {
            if pattern.is_match(line) {
                ctx.lines.push(i + 1); // 1-indexed
            }
        }

        true
    }

    /// Render the message template with context values.
    fn render_message(&self, ctx: &MatchContext) -> String {
        let mut message = self.message_template.clone();

        // {{ lines }}
        if !ctx.lines.is_empty() {
            let lines_str = ctx
                .lines
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            message = message.replace("{{ lines }}", &lines_str);
        } else {
            message = message.replace("{{ lines }}", "");
        }

        // {{ file_path }}
        if let Some(ref fp) = ctx.file_path {
            message = message.replace("{{ file_path }}", fp);
        } else {
            message = message.replace("{{ file_path }}", "");
        }

        // {{ matched }}
        if let Some(ref m) = ctx.matched {
            message = message.replace("{{ matched }}", m);
        } else {
            message = message.replace("{{ matched }}", "");
        }

        // {{ tool_name }}
        if let Some(ref tn) = ctx.tool_name {
            message = message.replace("{{ tool_name }}", tn);
        } else {
            message = message.replace("{{ tool_name }}", "");
        }

        // {{ prompt }}
        if let Some(ref p) = ctx.prompt {
            message = message.replace("{{ prompt }}", p);
        } else {
            message = message.replace("{{ prompt }}", "");
        }

        message
    }
}

/// Build a regex from a pattern.
///
/// Use inline flags for modifiers: `(?i)` for case-insensitive, `(?m)` for multiline, etc.
fn build_regex(pattern: &str) -> Result<Regex> {
    Regex::new(pattern).with_context(|| format!("invalid regex: {pattern}"))
}
