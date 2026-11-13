//! Folder browser the TUI opens to pick a server to import. Arrow keys walk folders, folders
//! that look like a Minecraft server are marked, and enter on one picks it.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Stylize},
    text::Line,
    widgets::{Block, Clear, List, ListItem, ListState},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// What a key press did.
pub enum Out {
    Stay,
    Close,
    Pick(PathBuf),
}

enum Kind {
    /// The folder being shown.
    Here,
    Up,
    Folder,
}

struct Row {
    kind: Kind,
    path: PathBuf,
    server: bool,
}

pub struct Browser {
    dir: PathBuf,
    rows: Vec<Row>,
    state: ListState,
    error: Option<String>,
}

/// What `beacon server import` accepts as a server folder.
fn is_server(dir: &Path) -> bool {
    dir.join("server.properties").is_file() || dir.join("mods").is_dir()
}

impl Browser {
    pub fn open(dir: PathBuf) -> Self {
        let mut b = Self { dir: PathBuf::new(), rows: vec![], state: ListState::default(), error: None };
        b.go(dir, None);
        b
    }

    /// Shows `dir`, selecting `select` (the folder we came up from) when given.
    fn go(&mut self, dir: PathBuf, select: Option<&Path>) {
        let mut rows = vec![Row { kind: Kind::Here, server: is_server(&dir), path: dir.clone() }];
        if let Some(up) = dir.parent() {
            rows.push(Row { kind: Kind::Up, path: up.to_path_buf(), server: false });
        }
        self.error = None;
        match fs::read_dir(&dir) {
            Ok(entries) => {
                let mut folders: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
                    .collect();
                folders.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
                rows.extend(folders.into_iter().map(|p| Row { kind: Kind::Folder, server: is_server(&p), path: p }));
            }
            Err(e) => self.error = Some(format!("can't read this folder: {e}")),
        }
        let at = select.and_then(|s| rows.iter().position(|r| r.path == s));
        // Start on the first server below, else on the folder itself.
        let first_server = rows.iter().position(|r| r.server && matches!(r.kind, Kind::Folder));
        self.state.select(Some(at.or(first_server).unwrap_or(0)));
        self.rows = rows;
        self.dir = dir;
    }

    fn up(&mut self) {
        if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
            let from = self.dir.clone();
            self.go(parent, Some(&from));
        }
    }

    pub fn key(&mut self, k: KeyEvent) -> Out {
        let sel = self.state.selected().unwrap_or(0).min(self.rows.len() - 1);
        let row = &self.rows[sel];
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') => return Out::Close,
            KeyCode::Down | KeyCode::Char('j') => self.state.select(Some((sel + 1).min(self.rows.len() - 1))),
            KeyCode::Up | KeyCode::Char('k') => self.state.select(Some(sel.saturating_sub(1))),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => self.up(),
            KeyCode::Char('~') => {
                if let Some(home) = std::env::var_os("HOME") {
                    self.go(home.into(), None);
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                let pick = k.code == KeyCode::Enter && row.server;
                match row.kind {
                    _ if pick => return Out::Pick(row.path.clone()),
                    Kind::Here if k.code == KeyCode::Enter => {
                        self.error = Some("Not a Minecraft server: no server.properties or mods folder".into())
                    }
                    Kind::Here => {}
                    Kind::Up => self.up(),
                    Kind::Folder => {
                        let p = row.path.clone();
                        self.go(p, None);
                    }
                }
            }
            _ => {}
        }
        Out::Stay
    }

    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let w = area.width.saturating_sub(4).min(80);
        let h = area.height.saturating_sub(4).min(24);
        let rect = Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h };
        f.render_widget(Clear, rect);
        let block = Block::bordered().title(" Import a server ".bold()).border_style(Color::White);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        let [path, list, note, keys] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
                .areas(inner);
        // Long paths keep their end, the part that tells folders apart.
        let full = self.dir.display().to_string();
        let room = path.width as usize;
        let shown = match full.chars().count() {
            n if n > room => format!("…{}", full.chars().skip(n + 1 - room).collect::<String>()),
            _ => full,
        };
        f.render_widget(Line::from(shown).dark_gray(), path);
        let items: Vec<ListItem> = self
            .rows
            .iter()
            .map(|r| {
                let name = match r.kind {
                    Kind::Here => "./  this folder".to_string(),
                    Kind::Up => "../".to_string(),
                    Kind::Folder => format!("{}/", r.path.file_name().unwrap_or_default().to_string_lossy()),
                };
                let mut line = vec![name.into()];
                if r.server {
                    line.push("  ● server".green());
                }
                ListItem::new(Line::from(line))
            })
            .collect();
        let mut state = self.state;
        f.render_stateful_widget(List::new(items).highlight_style(crate::tui::highlight(true)), list, &mut state);
        if let Some(e) = &self.error {
            f.render_widget(Line::from(e.clone()).yellow(), note);
        }
        let hints = [("enter", "import server / open"), ("←", "up"), ("→", "open"), ("~", "home"), ("esc", "cancel")];
        let spans: Vec<_> = hints.into_iter().flat_map(|(k, v)| [format!("{k} ").bold(), format!("{v}  ").dark_gray()]).collect();
        f.render_widget(Line::from(spans), keys);
    }
}
