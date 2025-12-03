//! Validate rule configuration files.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use clap::Args;
use color_eyre::eyre::{Context, Result};
use walkdir::WalkDir;

use nudge::rules::config::{load_rules_from, project_dirs};
use nudge::rules::eval::CompiledRule;
use nudge::rules::schema::{Action, HookType};

#[derive(Args, Clone, Debug)]
pub struct Config {
    /// Path to a specific config file to validate.
    /// If not specified, validates all discoverable config files.
    pub path: Option<PathBuf>,
}

pub fn main(config: Config) -> Result<()> {
    match config.path {
        Some(path) => validate_file(&path),
        None => validate_all(),
    }
}

/// Validate all discoverable config files.
fn validate_all() -> Result<()> {
    let mut found_any = false;

    if let Some(proj_dirs) = project_dirs() {
        let path = proj_dirs.config_dir().join("rules.yaml");
        if path.exists() {
            validate_file(&path)?;
            found_any = true;
        }
    }

    let project_file = Path::new(".nudge.yaml");
    if project_file.exists() {
        validate_file(project_file)?;
        found_any = true;
    }

    let nudge_dir = Path::new(".nudge");
    if nudge_dir.is_dir() {
        for entry in WalkDir::new(nudge_dir)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(OsStr::new("yaml")))
            .filter(|e| e.file_type().is_file())
        {
            validate_file(entry.path())?;
            found_any = true;
        }
    }

    if !found_any {
        println!("No config files found.");
    }

    Ok(())
}

/// Validate a single config file and print results.
fn validate_file(path: &Path) -> Result<()> {
    let rules =
        load_rules_from(path).with_context(|| format!("failed to validate {}", path.display()))?;

    // Try to compile each rule to catch regex/glob errors
    let mut compiled_rules = Vec::new();
    for rule in &rules {
        let compiled = CompiledRule::compile(rule.clone()).with_context(|| {
            format!(
                "failed to compile rule '{}' in {}",
                rule.name,
                path.display()
            )
        })?;
        compiled_rules.push((rule, compiled));
    }

    println!("{}: {} rules loaded", path.display(), rules.len());

    for (rule, _compiled) in &compiled_rules {
        let hook_type = format_hook_type(&rule.on.hook);
        let action = format_action(&rule.action);
        println!("  - {} ({}, {})", rule.name, hook_type, action);
    }

    Ok(())
}

fn format_hook_type(hook: &HookType) -> &'static str {
    match hook {
        HookType::PreToolUse => "PreToolUse",
        HookType::PostToolUse => "PostToolUse",
        HookType::UserPromptSubmit => "UserPromptSubmit",
        HookType::Stop => "Stop",
    }
}

fn format_action(action: &Action) -> &'static str {
    match action {
        Action::Interrupt => "interrupt",
        Action::Continue => "continue",
    }
}
