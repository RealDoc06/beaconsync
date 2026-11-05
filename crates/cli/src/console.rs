//! The server screen of the TUI: live console with a command line, and backups.
//!
//! Opened from the TUI's Servers tab or with `beacon server console`. It talks to the server
//! through the hub, so it works from any machine the owner is signed in on.

use beacon_core::{
    api::{Backup, ConsoleView, Server, ServerAction},
    hub::Hub,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Lines kept on screen.
const KEEP: usize = 5000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Console,
    Backups,
}

/// What the host should do after a key.
pub enum Out {
    None,
    Close,
    Act(ServerAction),
    /// Ask first, then send the action.
    Confirm(String, ServerAction),
}

pub struct ServerScreen {
    pub hub: Hub,
    pub server: Server,
    /// Release the server runs, e.g. "1.6".
    version: String,
    lines: Vec<String>,
    backups: Vec<Backup>,
    online: bool,
    running: bool,
    loaded: bool,
    error: Option<String>,
    input: String,
    history: Vec<String>,
    /// Position while browsing history with up/down.
    hist: Option<usize>,
    pane: Pane,
    sel: ListState,
    /// Lines scrolled up from the bottom; 0 follows new output.
    scroll: usize,
    stop: Arc<AtomicBool>,
}

impl Drop for ServerScreen {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl ServerScreen {
    /// Opens the screen and starts long polling the console; `send` delivers each update and
    /// returns false once the receiver is gone.
    pub fn open(
        hub: Hub,
        server: Server,
        version: String,
        send: impl Fn(Result<ConsoleView, String>) -> bool + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (h, id, flag) = (hub.clone(), server.id.clone(), stop.clone());
        tokio::spawn(async move {
            let mut next = None;
            while !flag.load(Ordering::Relaxed) {
                match h.console(&id, next, 20).await {
                    Ok(v) => {
                        next = Some(v.next);
                        if flag.load(Ordering::Relaxed) || !send(Ok(v)) {
                            return;
                        }
                    }
                    Err(e) => {
                        if !send(Err(format!("{e:#}"))) {
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                }
            }
        });
        Self {
            hub,
            server,
            version,
            lines: vec![],
            backups: vec![],
            online: false,
            running: false,
            loaded: false,
            error: None,
            input: String::new(),
            history: vec![],
            hist: None,
            pane: Pane::Console,
            sel: ListState::default(),
            scroll: 0,
            stop,
        }
    }

    pub fn apply(&mut self, r: Result<ConsoleView, String>) {
        match r {
            Ok(v) => {
                if self.scroll > 0 {
                    self.scroll += v.lines.len();
                }
                self.lines.extend(v.lines);
                let extra = self.lines.len().saturating_sub(KEEP);
                self.lines.drain(..extra);
                self.backups = v.backups;
                self.online = v.online;
                self.running = v.running;
                self.loaded = true;
                self.error = None;
                let len = self.backups.len();
                match self.sel.selected() {
                    _ if len == 0 => self.sel.select(None),
                    Some(i) if i >= len => self.sel.select(Some(len - 1)),
                    None => self.sel.select(Some(0)),
                    _ => {}
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn power(&self, k: KeyEvent) -> Option<Out> {
        if !k.modifiers.contains(KeyModifiers::CONTROL) {
            return None;
        }
        let name = &self.server.name;
        match k.code {
            KeyCode::Char('r') => Some(Out::Confirm(format!("Restart {name}? Players are disconnected."), ServerAction::Restart)),
            KeyCode::Char('s') if self.running => {
                Some(Out::Confirm(format!("Stop Minecraft on {name}? It stays stopped until you start it."), ServerAction::Stop))
            }
            KeyCode::Char('s') => Some(Out::Act(ServerAction::Start)),
            _ => None,
        }
    }

    pub fn key(&mut self, k: KeyEvent) -> Out {
        if let Some(o) = self.power(k) {
            return o;
        }
        match (self.pane, k.code) {
            (_, KeyCode::Esc) => Out::Close,
            (_, KeyCode::Tab | KeyCode::BackTab) => {
                self.pane = if self.pane == Pane::Console { Pane::Backups } else { Pane::Console };
                Out::None
            }
            (Pane::Console, KeyCode::Enter) => {
                let c = std::mem::take(&mut self.input).trim().to_string();
                self.hist = None;
                self.scroll = 0;
                if c.is_empty() {
                    return Out::None;
                }
                if self.history.last() != Some(&c) {
                    self.history.push(c.clone());
                }
                Out::Act(ServerAction::Command(c.trim_start_matches('/').to_string()))
            }
            (Pane::Console, KeyCode::Up | KeyCode::Down) if !self.history.is_empty() => {
                let last = self.history.len() - 1;
                self.hist = match (self.hist, k.code) {
                    (None, KeyCode::Up) => Some(last),
                    (Some(i), KeyCode::Up) => Some(i.saturating_sub(1)),
                    (Some(i), KeyCode::Down) if i < last => Some(i + 1),
                    _ => None,
                };
                self.input = self.hist.map(|i| self.history[i].clone()).unwrap_or_default();
                Out::None
            }
            (Pane::Console, KeyCode::PageUp) => {
                self.scroll = (self.scroll + 10).min(self.lines.len().saturating_sub(1));
                Out::None
            }
            (Pane::Console, KeyCode::PageDown) => {
                self.scroll = self.scroll.saturating_sub(10);
                Out::None
            }
            (Pane::Console, KeyCode::End) => {
                self.scroll = 0;
                Out::None
            }
            (Pane::Console, KeyCode::Backspace) => {
                self.input.pop();
                Out::None
            }
            (Pane::Console, KeyCode::Char(c)) => {
                self.input.push(c);
                Out::None
            }
            (Pane::Backups, KeyCode::Down | KeyCode::Char('j')) => {
                let len = self.backups.len();
                if len > 0 {
                    self.sel.select(Some(self.sel.selected().map_or(0, |i| (i + 1).min(len - 1))));
                }
                Out::None
            }
            (Pane::Backups, KeyCode::Up | KeyCode::Char('k')) => {
                self.sel.select(self.sel.selected().map(|i| i.saturating_sub(1)));
                Out::None
            }
            (Pane::Backups, KeyCode::Char('b')) => Out::Act(ServerAction::Backup),
            (Pane::Backups, KeyCode::Char('r') | KeyCode::Enter) => match self.selected() {
                Some(b) => {
                    let what = if b.full { "the whole server" } else { "the world" };
                    Out::Confirm(
                        format!(
                            "Restore {what} from {} ({})? Minecraft restarts. A safety backup is taken first.",
                            crate::ui::ago(b.created),
                            b.source
                        ),
                        ServerAction::Restore(b.id.clone()),
                    )
                }
                None => Out::None,
            },
            (Pane::Backups, KeyCode::Char('x')) => match self.selected() {
                Some(b) => {
                    Out::Confirm(format!("Delete the {} backup {}?", b.source, b.note), ServerAction::DeleteBackup(b.id.clone()))
                }
                None => Out::None,
            },
            _ => Out::None,
        }
    }

    fn selected(&self) -> Option<&Backup> {
        self.sel.selected().and_then(|i| self.backups.get(i))
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let power = if self.running { ("^s", "stop") } else { ("^s", "start") };
        match self.pane {
            Pane::Console => vec![
                ("enter", "send"),
                ("↑", "history"),
                ("pgup", "scroll"),
                ("tab", "backups"),
                power,
                ("^r", "restart"),
                ("esc", "back"),
            ],
            Pane::Backups => {
                vec![("b", "back up now"), ("r", "restore"), ("x", "delete"), ("tab", "console"), power, ("esc", "back")]
            }
        }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        let [head, tabs, body] = Layout::vertical([Constraint::Length(2), Constraint::Length(2), Constraint::Min(1)]).areas(area);
        let state = match (self.loaded, self.online, self.running) {
            (false, ..) => "connecting".dark_gray(),
            (_, false, _) => "○ offline".dark_gray(),
            (_, true, true) => "● running".green(),
            (_, true, false) => "● stopped".yellow(),
        };
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![self.server.name.clone().bold(), "  ".into(), state]),
                Line::from(match &self.error {
                    Some(e) => e.clone().red(),
                    None => format!(
                        "{} · {} · {}",
                        self.server.address,
                        format!("{:?}", self.server.mode).to_lowercase(),
                        self.version
                    )
                    .dark_gray(),
                }),
            ]),
            head,
        );
        let tab = |p: Pane, s: &'static str| if self.pane == p { s.white().bold().underlined() } else { s.dark_gray() };
        let count = format!(" ({})", self.backups.len());
        f.render_widget(
            Line::from(vec![tab(Pane::Console, "Console"), "   ".into(), tab(Pane::Backups, "Backups"), count.dark_gray()]),
            tabs,
        );
        match self.pane {
            Pane::Console => self.draw_console(f, body),
            Pane::Backups => self.draw_backups(f, body),
        }
    }

    fn draw_console(&self, f: &mut Frame, area: Rect) {
        let [log, input] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        let h = log.height as usize;
        let end = self.lines.len().saturating_sub(self.scroll);
        let shown: Vec<Line> = self.lines[end.saturating_sub(h)..end].iter().map(|l| style_line(l)).collect();
        f.render_widget(Paragraph::new(shown), log);
        let prompt = if self.running { "> ".white().bold() } else { "> ".dark_gray() };
        let mut spans = vec![prompt, self.input.clone().into(), " ".reversed()];
        if self.scroll > 0 {
            spans.push(format!("   {} lines up · end to follow", self.scroll).dark_gray());
        }
        f.render_widget(Line::from(spans), input);
    }

    fn draw_backups(&mut self, f: &mut Frame, area: Rect) {
        if self.backups.is_empty() {
            let text = if self.loaded { "No backups yet. Press b to back up the server now." } else { "Loading..." };
            return f.render_widget(Paragraph::new(text).dark_gray(), area);
        }
        let items: Vec<ListItem> = self
            .backups
            .iter()
            .map(|b| {
                let what = if b.full { "whole server" } else { "world" };
                ListItem::new(Line::from(vec![
                    format!("{:<16}", b.source).into(),
                    format!("{:<10}", crate::ui::ago(b.created)).into(),
                    format!("{:>9}  ", crate::server::size(b.size)).dark_gray(),
                    format!("{:<13}", what).dark_gray(),
                    b.note.clone().into(),
                ]))
            })
            .collect();
        let list = List::new(items).highlight_style(Style::new().bg(Color::Indexed(237)));
        f.render_stateful_widget(list, area, &mut self.sel);
    }
}

/// Colors a console line: our own messages, typed commands, warnings and errors.
fn style_line(l: &str) -> Line<'static> {
    let s: Span = if l.starts_with("[beacon]") {
        l.to_string().cyan()
    } else if l.starts_with("> ") {
        l.to_string().white().bold()
    } else if l.contains("/ERROR]") || l.contains(" ERROR ") {
        l.to_string().red()
    } else if l.contains("/WARN]") || l.contains(" WARN ") {
        l.to_string().yellow()
    } else {
        l.to_string().into()
    };
    Line::from(s)
}
