//! Terminal helpers: progress bars, prompts and colors.

use beacon_core::fetch::Progress;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

/// Progress bar fed by core `Progress` events.
pub struct Bar(ProgressBar);

impl Bar {
    pub fn new() -> Self {
        let pb = ProgressBar::new(0);
        pb.set_style(ProgressStyle::with_template("{bar:30.white/black} {pos}/{len} {wide_msg}").unwrap().progress_chars("█▉ "));
        Self(pb)
    }

    pub fn update(&self, p: Progress) {
        self.0.set_length(p.total as u64);
        self.0.set_position(p.done as u64);
        self.0.set_message(p.label);
    }

    pub fn done(self) {
        self.0.finish_and_clear();
    }
}

impl Default for Bar {
    fn default() -> Self {
        Self::new()
    }
}

pub fn ok(msg: impl std::fmt::Display) {
    println!("{} {msg}", style("✓").green().bold());
}

pub fn info(msg: impl std::fmt::Display) {
    println!("{} {msg}", style("›").dim());
}

pub fn warn(msg: impl std::fmt::Display) {
    eprintln!("{} {msg}", style("!").yellow().bold());
}

pub fn dim(s: impl std::fmt::Display) -> String {
    style(s).dim().to_string()
}

pub fn bold(s: impl std::fmt::Display) -> String {
    style(s).bold().to_string()
}

static THEME: dialoguer::theme::SimpleTheme = dialoguer::theme::SimpleTheme;

pub fn theme() -> &'static dialoguer::theme::SimpleTheme {
    &THEME
}

pub fn input(prompt: &str, default: Option<String>) -> anyhow::Result<String> {
    let mut i = dialoguer::Input::<String>::with_theme(theme()).with_prompt(prompt);
    if let Some(d) = default {
        i = i.default(d);
    }
    Ok(i.interact_text()?)
}

pub fn select(prompt: &str, items: &[String], default: usize) -> anyhow::Result<usize> {
    Ok(dialoguer::Select::with_theme(theme()).with_prompt(prompt).items(items).default(default).interact()?)
}

pub fn confirm(prompt: &str, default: bool) -> anyhow::Result<bool> {
    Ok(dialoguer::Confirm::with_theme(theme()).with_prompt(prompt).default(default).interact()?)
}

pub fn password(prompt: &str) -> anyhow::Result<String> {
    Ok(dialoguer::Password::with_theme(theme()).with_prompt(prompt).interact()?)
}

pub fn ago(t: i64) -> String {
    let d = beacon_core::now() - t;
    match d {
        ..60 => "just now".into(),
        60..3600 => format!("{}m ago", d / 60),
        3600..86400 => format!("{}h ago", d / 3600),
        _ => format!("{}d ago", d / 86400),
    }
}
