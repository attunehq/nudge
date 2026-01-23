//! Responds to Claude Code hooks.

use std::io;
use std::iter::repeat;

use clap::Args;
use color_eyre::{Result, eyre::Context};
use indoc::formatdoc;
use itertools::Itertools;
use nudge::{
    claude::hook::{
        Hook, PreToolUseBashPayload, PreToolUseEditPayload, PreToolUseInterruptResponse,
        PreToolUsePayload, PreToolUseWebFetchPayload, PreToolUseWritePayload,
        UserPromptSubmitPayload, UserPromptSubmitResponse,
    },
    rules::{self, Rule},
    snippet::{Annotation, Source},
};
use tap::Pipe;
use tracing::instrument;

#[derive(Args, Clone, Debug)]
pub struct Config {}

#[instrument]
pub fn main(_config: Config) -> Result<()> {
    let stdin = io::stdin();
    let hook = serde_json::from_reader::<_, Hook>(stdin).context("read hook event")?;

    let rules = rules::load_all().context("load rules")?;
    match hook {
        Hook::PreToolUse(payload) => main_pretooluse(payload, &rules),
        Hook::UserPromptSubmit(payload) => main_userpromptsubmit(payload, &rules),
        Hook::Other => Ok(()), // Passthrough for unhandled hook types
    }
}

fn main_pretooluse(payload: PreToolUsePayload, rules: &[Rule]) -> Result<()> {
    match payload {
        PreToolUsePayload::Write(payload) => main_pretooluse_write(payload, rules),
        PreToolUsePayload::Edit(payload) => main_pretooluse_edit(payload, rules),
        PreToolUsePayload::WebFetch(payload) => main_pretooluse_webfetch(payload, rules),
        PreToolUsePayload::Bash(payload) => main_pretooluse_bash(payload, rules),
        PreToolUsePayload::Other => Ok(()), // Passthrough for unhandled tool types
    }
}

fn main_pretooluse_write(payload: PreToolUseWritePayload, rules: &[Rule]) -> Result<()> {
    rules
        .iter()
        .flat_map(|rule| repeat(rule).zip(rule.hooks_pretooluse_write()))
        .flat_map(|(rule, matcher)| rule.annotate_matches(payload.evaluate(matcher)))
        .collect_vec()
        .pipe(|matches| respond_pretooluse(payload.tool_input.content, matches))
}

fn main_pretooluse_edit(payload: PreToolUseEditPayload, rules: &[Rule]) -> Result<()> {
    rules
        .iter()
        .flat_map(|rule| repeat(rule).zip(rule.hooks_pretooluse_edit()))
        .flat_map(|(rule, matcher)| rule.annotate_matches(payload.evaluate(matcher)))
        .collect_vec()
        .pipe(|matches| respond_pretooluse(payload.tool_input.new_string, matches))
}

fn main_pretooluse_webfetch(payload: PreToolUseWebFetchPayload, rules: &[Rule]) -> Result<()> {
    rules
        .iter()
        .flat_map(|rule| repeat(rule).zip(rule.hooks_pretooluse_webfetch()))
        .flat_map(|(rule, matcher)| rule.annotate_matches(payload.evaluate(matcher)))
        .collect_vec()
        .pipe(|matches| respond_pretooluse(payload.tool_input.url, matches))
}

fn main_pretooluse_bash(payload: PreToolUseBashPayload, rules: &[Rule]) -> Result<()> {
    rules
        .iter()
        .flat_map(|rule| repeat(rule).zip(rule.hooks_pretooluse_bash()))
        .flat_map(|(rule, matcher)| rule.annotate_matches(payload.evaluate(matcher)))
        .collect_vec()
        .pipe(|matches| respond_pretooluse(payload.tool_input.command, matches))
}

fn respond_pretooluse(content: impl Into<Source>, annotations: Vec<Annotation>) -> Result<()> {
    if !annotations.is_empty() {
        let matches = content.into().annotate(annotations);
        let message = formatdoc! {"
            Nudge blocked operation due to rule violation.
            Fix all issues immediately and try again:

            {matches}
        "};
        let response = PreToolUseInterruptResponse::builder()
            .model_feedback(&message)
            .user_message(message)
            .build()
            .pipe_ref(serde_json::to_string)
            .context("serialize response")?;
        println!("{response}");
    }

    Ok(())
}

fn main_userpromptsubmit(payload: UserPromptSubmitPayload, rules: &[Rule]) -> Result<()> {
    rules
        .iter()
        .flat_map(|rule| repeat(rule).zip(rule.hooks_userpromptsubmit()))
        .flat_map(|(rule, matcher)| rule.annotate_matches(payload.evaluate(matcher)))
        .collect_vec()
        .pipe(|matches| respond_userpromptsubmit(payload.prompt, matches))
}

fn respond_userpromptsubmit(prompt: impl Into<Source>, annotations: Vec<Annotation>) -> Result<()> {
    if !annotations.is_empty() {
        let matches = prompt.into().annotate(annotations);
        let response = UserPromptSubmitResponse::from(matches);
        println!("{response}");
    }
    Ok(())
}
