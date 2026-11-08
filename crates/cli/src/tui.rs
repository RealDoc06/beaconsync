//! Full screen terminal UI, opened by running `beacon` with no command.
//!
//! Left: the packs on this machine. Right: the selected pack's changes, history, branches
//! and releases with live server status, and servers you can open for their console and
//! backups (`console.rs`). Work runs on tokio tasks that report back over one channel, so the
//! screen never blocks.

use crate::{
    console::{Out, ServerScreen},
    pack::{author, hub_for},
};
use anyhow::{Context, Result};
use beacon_core::{
    api::{AllServers, ConsoleView, DeployRequest, DeployTargets, Release, Server, ServerAction},
    diff::ChangeKind,
    fetch::Progress,
    hash::short,
    hub::Hub,
    instance::{self, InstanceInfo},
    ops, prism,
    repo::{BranchInfo, LogEntry, PackConfig, Repo, Role, Status},
    settings::Settings,
};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph},
};
use std::{future::Future, path::PathBuf, time::Duration};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Changes,
    History,
    Branches,
    Releases,
    Servers,
}

const TABS: [Tab; 5] = [Tab::Changes, Tab::History, Tab::Branches, Tab::Releases, Tab::Servers];

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Changes => "Changes",
            Tab::History => "History",
            Tab::Branches => "Branches",
            Tab::Releases => "Releases",
            Tab::Servers => "Servers",
        }
    }
}

/// The selected pack as read from disk.
struct Local {
    repo: Repo,
    cfg: PackConfig,
    status: Status,
    log: Vec<LogEntry>,
    branches: Vec<BranchInfo>,
}

/// The selected pack as the hub sees it.
struct Remote {
    releases: Vec<Release>,
    servers: Vec<ServerRow>,
}

struct ServerRow {
    /// Full server info for owners, who can open it; players only see the public fields.
    owned: Option<Server>,
    name: String,
    address: String,
    online: bool,
    commit: Option<String>,
    /// Commit a deploy is moving the server to.
    target: Option<String>,
    message: String,
}

enum Msg {
    Key(KeyEvent),
    Redraw,
    Local(PathBuf, Box<Result<Local>>),
    Remote(PathBuf, Result<Remote>),
    Progress(Progress),
    Done(Result<String>),
    /// A result to show that needs no reload, e.g. a sent server action.
    Note(Result<String>),
    Console(Result<ConsoleView, String>),
}

/// Text prompts.
#[derive(Clone, Copy)]
enum Ask {
    Save,
    Publish,
    Branch,
}

/// Yes/no prompts.
enum Sure {
    Discard,
    Rollback { id: String },
    Deploy { release: Release },
    Server(ServerAction),
}

enum Modal {
    Input {
        ask: Ask,
        value: String,
    },
    /// `everyone` is the "also roll back players and servers" toggle, when it applies.
    Confirm {
        sure: Sure,
        text: String,
        everyone: Option<bool>,
    },
}

struct App {
    tx: UnboundedSender<Msg>,
    packs: Vec<InstanceInfo>,
    pack: ListState,
    /// Keys go to the right pane when true.
    inside: bool,
    tab: Tab,
    rows: [ListState; 5],
    /// The open server screen, drawn instead of the pack.
    server: Option<ServerScreen>,
    /// Opened with `beacon server console`: closing the server quits.
    server_only: bool,
    local: Option<Local>,
    remote: Option<Result<Remote, String>>,
    modal: Option<Modal>,
    busy: Option<(String, Option<Progress>)>,
    note: Option<(bool, String)>,
    account: String,
    quit: bool,
}

/// The app channel, fed with key presses from a reader thread.
fn keys_channel() -> (UnboundedSender<Msg>, UnboundedReceiver<Msg>) {
    let (tx, rx) = unbounded_channel();
    let keys = tx.clone();
    std::thread::spawn(move || {
        loop {
            let msg = match event::read() {
                Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => Msg::Key(k),
                Ok(Event::Resize(..)) => Msg::Redraw,
                Ok(_) => continue,
                Err(_) => break,
            };
            if keys.send(msg).is_err() {
                break;
            }
        }
    });
    (tx, rx)
}

/// Runs the UI until the user quits.
pub async fn run() -> Result<()> {
    let (tx, rx) = keys_channel();
    let app = App::new(tx, true);
    event_loop(app, rx).await
}

/// Opens straight into one server's console (`beacon server console`).
pub async fn run_server(hub: Hub, server: Server, version: String) -> Result<()> {
    let (tx, rx) = keys_channel();
    let mut app = App::new(tx, false);
    app.server_only = true;
    app.open_server(hub, server, version);
    event_loop(app, rx).await
}

async fn event_loop(mut app: App, mut rx: UnboundedReceiver<Msg>) -> Result<()> {
    let mut term = ratatui::init();
    let mut tick = tokio::time::interval(Duration::from_secs(3));
    let res = loop {
        if let Err(e) = term.draw(|f| app.draw(f)) {
            break Err(e.into());
        }
        tokio::select! {
            Some(m) = rx.recv() => app.on(m),
            _ = tick.tick() => app.tick(),
        }
        if app.quit {
            break Ok(());
        }
    };
    ratatui::restore();
    res
}

fn release_of<'a>(remote: &'a Option<Result<Remote, String>>, commit: &str) -> Option<&'a str> {
    let r = remote.as_ref()?.as_ref().ok()?;
    r.releases.iter().find(|x| x.commit == commit).map(|x| x.name.as_str())
}

fn read_local(path: PathBuf) -> Result<Local> {
    let repo = Repo::open(&path)?;
    let cfg = repo.config()?;
    let status = repo.status()?;
    let log = match repo.head_commit()? {
        Some(h) => repo.log(&h, 200)?,
        None => vec![],
    };
    let branches = repo.branches()?;
    Ok(Local { repo, cfg, status, log, branches })
}

async fn read_remote(repo: Repo) -> Result<Remote> {
    let cfg = repo.config()?;
    let remote = cfg.remote.context("not published")?;
    if cfg.role == Role::Owner {
        let hub = hub_for(&repo)?;
        let (releases, servers) = tokio::try_join!(hub.releases(&remote.pack), hub.servers(&remote.pack))?;
        let servers = servers
            .into_iter()
            .map(|s| ServerRow {
                owned: Some(s.clone()),
                name: s.name,
                address: s.address,
                online: s.online,
                commit: s.current_commit,
                target: s.target_commit.filter(|_| matches!(s.status.as_str(), "pending" | "applying")),
                message: if s.status == "failed" { s.message } else { String::new() },
            })
            .collect();
        return Ok(Remote { releases, servers });
    }
    let p = Hub::new(&remote.hub, None).public_pack(&remote.code).await?;
    let servers = p
        .servers
        .into_iter()
        .map(|s| ServerRow {
            owned: None,
            name: s.name,
            address: s.address,
            online: s.online,
            commit: s.commit,
            target: s.target.filter(|_| s.updating),
            message: String::new(),
        })
        .collect();
    Ok(Remote { releases: p.latest.into_iter().collect(), servers })
}

impl App {
    /// `packs` loads the pack list (false for the server only screen).
    fn new(tx: UnboundedSender<Msg>, packs: bool) -> Self {
        let s = Settings::load();
        let account = match (&s.token, &s.username) {
            (Some(_), Some(u)) => format!("{u} @ {}", s.hub_url()),
            _ => "not signed in".into(),
        };
        let mut app = Self {
            tx,
            packs: vec![],
            pack: ListState::default(),
            inside: false,
            tab: Tab::Changes,
            rows: Default::default(),
            server: None,
            server_only: false,
            local: None,
            remote: None,
            modal: None,
            busy: None,
            note: None,
            account,
            quit: false,
        };
        if packs {
            app.reload_packs();
        }
        app
    }

    fn open_server(&mut self, hub: Hub, server: Server, version: String) {
        let tx = self.tx.clone();
        self.note = None;
        self.server = Some(ServerScreen::open(hub, server, version, move |v| tx.send(Msg::Console(v)).is_ok()));
    }

    /// Sends an action to the open server; its output shows up in the console.
    fn send_action(&mut self, a: ServerAction) {
        let Some(scr) = &self.server else { return };
        let (hub, id, tx) = (scr.hub.clone(), scr.server.id.clone(), self.tx.clone());
        let label = match &a {
            ServerAction::Command(_) => None,
            ServerAction::Start => Some("Starting"),
            ServerAction::Stop => Some("Stopping"),
            ServerAction::Restart => Some("Restarting"),
            ServerAction::Backup => Some("Backing up, follow along in the console"),
            ServerAction::Restore(_) => Some("Restoring, follow along in the console"),
            ServerAction::DeleteBackup(_) => Some("Deleting"),
            ServerAction::Update(_) => Some("Updating beacon, follow along in the console"),
        };
        tokio::spawn(async move {
            let r = hub.server_action(&id, &a).await.map(|_| label.unwrap_or_default().to_string());
            tx.send(Msg::Note(r)).ok();
        });
    }

    fn reload_packs(&mut self) {
        self.packs = instance::list(&Settings::load().instance_roots()).into_iter().filter(|i| i.tracked).collect();
        let sel = self.pack.selected().unwrap_or(0).min(self.packs.len().saturating_sub(1));
        self.pack.select((!self.packs.is_empty()).then_some(sel));
        self.load(true);
    }

    fn path(&self) -> Option<PathBuf> {
        self.pack.selected().and_then(|i| self.packs.get(i)).map(|p| p.path.clone())
    }

    /// Reads the selected pack again; `fresh` clears what is shown first (pack changed).
    fn load(&mut self, fresh: bool) {
        let Some(path) = self.path() else { return };
        if fresh {
            self.local = None;
            self.remote = None;
            self.rows = Default::default();
        }
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let r = read_local(path.clone());
            let repo = r.as_ref().ok().filter(|l| l.cfg.remote.is_some()).map(|l| l.repo.clone());
            tx.send(Msg::Local(path.clone(), Box::new(r))).ok();
            if let Some(repo) = repo {
                tokio::runtime::Handle::current().spawn(async move {
                    tx.send(Msg::Remote(path, read_remote(repo).await)).ok();
                });
            }
        });
    }

    fn tick(&mut self) {
        // Servers move during a deploy, so the Releases tab polls the hub.
        let live = matches!(self.tab, Tab::Releases | Tab::Servers);
        if self.busy.is_none() && self.server.is_none() && live && self.local.as_ref().is_some_and(|l| l.cfg.remote.is_some()) {
            self.load(false);
        }
    }

    /// Starts `work` in the background; its message shows in the footer when it finishes.
    fn spawn<F, Fut>(&mut self, label: &str, work: F)
    where
        F: FnOnce(UnboundedSender<Msg>) -> Fut,
        Fut: Future<Output = Result<String>> + Send + 'static,
    {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some((label.into(), None));
        self.note = None;
        let tx = self.tx.clone();
        let fut = work(tx.clone());
        tokio::spawn(async move {
            tx.send(Msg::Done(fut.await)).ok();
        });
    }

    fn on(&mut self, m: Msg) {
        match m {
            Msg::Key(k) => self.key(k),
            Msg::Redraw => {}
            Msg::Local(p, r) if Some(&p) == self.path().as_ref() => match *r {
                Ok(l) => {
                    for (i, len) in [l.status.diff.files.len(), l.log.len(), l.branches.len()].into_iter().enumerate() {
                        clamp(&mut self.rows[i], len);
                    }
                    self.local = Some(l);
                }
                Err(e) => self.note = Some((false, format!("{e:#}"))),
            },
            Msg::Remote(p, r) if Some(&p) == self.path().as_ref() => {
                if let Ok(r) = &r {
                    clamp(&mut self.rows[3], r.releases.len());
                    clamp(&mut self.rows[4], r.servers.len());
                }
                self.remote = Some(r.map_err(|e| format!("{e:#}")));
            }
            Msg::Local(..) | Msg::Remote(..) => {}
            Msg::Progress(p) => {
                if let Some(b) = &mut self.busy {
                    b.1 = Some(p);
                }
            }
            Msg::Note(r) => {
                self.note = match r {
                    Ok(s) if s.is_empty() => None,
                    Ok(s) => Some((true, s)),
                    Err(e) => Some((false, format!("{e:#}"))),
                }
            }
            Msg::Console(v) => {
                if let Some(scr) = &mut self.server {
                    scr.apply(v);
                }
            }
            Msg::Done(r) => {
                self.busy = None;
                self.note = Some(match r {
                    Ok(s) => (true, s),
                    Err(e) => (false, format!("{e:#}")),
                });
                self.reload_packs();
            }
        }
    }

    fn key(&mut self, k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.modal.is_some() {
            return self.modal_key(k);
        }
        if let Some(scr) = &mut self.server {
            match scr.key(k) {
                Out::None => {}
                Out::Close if self.server_only => self.quit = true,
                Out::Close => self.server = None,
                Out::Act(a) => self.send_action(a),
                Out::Confirm(text, a) => self.modal = Some(Modal::Confirm { sure: Sure::Server(a), text, everyone: None }),
            }
            return;
        }
        let by = match k.code {
            KeyCode::Down | KeyCode::Char('j') => 1,
            KeyCode::Up | KeyCode::Char('k') => -1,
            _ => 0,
        };
        if by != 0 {
            if self.inside {
                let len = self.list_len();
                step(&mut self.rows[self.tab as usize], len, by);
            } else {
                step(&mut self.pack, self.packs.len(), by);
                self.load(true);
            }
            return;
        }
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc if !self.inside => self.quit = true,
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => self.inside = false,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') if !self.inside => self.inside = true,
            KeyCode::Tab => self.tab = TABS[(self.tab as usize + 1) % TABS.len()],
            KeyCode::BackTab => self.tab = TABS[(self.tab as usize + TABS.len() - 1) % TABS.len()],
            KeyCode::Char(c @ '1'..='5') => self.tab = TABS[c as usize - '1' as usize],
            KeyCode::Char('R') => self.reload_packs(),
            KeyCode::Char(c) => self.action(c),
            KeyCode::Enter if matches!(self.tab, Tab::Branches | Tab::Servers) => self.action('\n'),
            _ => {}
        }
    }

    fn list_len(&self) -> usize {
        let Some(l) = &self.local else { return 0 };
        match self.tab {
            Tab::Changes => l.status.diff.files.len(),
            Tab::History => l.log.len(),
            Tab::Branches => l.branches.len(),
            Tab::Releases => self.remote.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |r| r.releases.len()),
            Tab::Servers => self.remote.as_ref().and_then(|r| r.as_ref().ok()).map_or(0, |r| r.servers.len()),
        }
    }

    fn selected<T>(&self, items: &'_ [T]) -> Option<usize> {
        self.rows[self.tab as usize].selected().filter(|&i| i < items.len())
    }

    /// Letter key actions for the selected pack.
    fn action(&mut self, c: char) {
        let Some(l) = &self.local else { return };
        let repo = l.repo.clone();
        let published = l.cfg.remote.is_some();
        let owner = l.cfg.role == Role::Owner;
        match (c, self.tab) {
            ('P', _) => {
                let path = repo.root.clone();
                self.spawn("Launching", move |tx| async move {
                    let server = ops::server_address(&repo).await;
                    let on = move |p| drop(tx.send(Msg::Progress(p)));
                    drop(prism::launch(&path, &Settings::load(), server.as_deref(), &on).await?);
                    Ok("Minecraft is starting".into())
                });
            }
            ('u', _) if l.cfg.role == Role::Subscriber => self.spawn("Updating", move |tx| async move {
                let info = ops::update_subscribed(&repo, &move |p| drop(tx.send(Msg::Progress(p)))).await?;
                Ok(format!("Up to date ({})", info.name))
            }),
            ('s', _) if !l.status.diff.is_empty() => self.modal = Some(Modal::Input { ask: Ask::Save, value: String::new() }),
            ('x', Tab::Changes) if !l.status.diff.is_empty() => {
                let text = format!("Throw away {} unsaved change(s)?", l.status.diff.files.len());
                self.modal = Some(Modal::Confirm { sure: Sure::Discard, text, everyone: None });
            }
            ('r', Tab::History) if l.cfg.role != Role::Subscriber => {
                let Some(e) = self.selected(&l.log).map(|i| &l.log[i]) else { return };
                let label = release_of(&self.remote, &e.id).unwrap_or(&e.commit.message);
                let text = format!("Roll back to \"{label}\"? It is saved as a new version.");
                let everyone = (owner && published).then_some(true);
                self.modal = Some(Modal::Confirm { sure: Sure::Rollback { id: e.id.clone() }, text, everyone });
            }
            ('\n', Tab::Servers) => {
                let Some(Ok(r)) = &self.remote else { return };
                let Some(row) = self.selected(&r.servers).map(|i| &r.servers[i]) else { return };
                let Some(server) = row.owned.clone() else {
                    self.note = Some((false, "Only the pack's owner can manage its servers".into()));
                    return;
                };
                let version = server
                    .current_commit
                    .as_deref()
                    .map(|c| release_of(&self.remote, c).map_or_else(|| short(c).to_string(), str::to_string))
                    .unwrap_or_default();
                match hub_for(&repo) {
                    Ok(hub) => self.open_server(hub, server, version),
                    Err(e) => self.note = Some((false, format!("{e:#}"))),
                }
            }
            ('n', Tab::Branches) => self.modal = Some(Modal::Input { ask: Ask::Branch, value: String::new() }),
            ('\n', Tab::Branches) => {
                let Some(b) = self.selected(&l.branches).map(|i| l.branches[i].name.clone()) else { return };
                self.spawn("Switching", move |tx| async move {
                    repo.switch(&b, false, &move |p| drop(tx.send(Msg::Progress(p)))).await?;
                    Ok(format!("Switched to {b}"))
                });
            }
            ('p', _) if owner || l.cfg.role == Role::Local => {
                self.modal = Some(Modal::Input { ask: Ask::Publish, value: String::new() })
            }
            ('d', Tab::Releases) if owner => {
                let Some(Ok(r)) = &self.remote else { return };
                let Some(release) = self.selected(&r.releases).map(|i| r.releases[i].clone()) else { return };
                let text = format!("Deploy {} to all {} server(s)?", release.name, r.servers.len());
                self.modal = Some(Modal::Confirm { sure: Sure::Deploy { release }, text, everyone: None });
            }
            _ => {}
        }
    }

    fn modal_key(&mut self, k: KeyEvent) {
        let Some(modal) = self.modal.take() else { return };
        // Server confirmations need no pack (`beacon server console` has none).
        if let Modal::Confirm { sure: Sure::Server(a), text, .. } = modal {
            match k.code {
                KeyCode::Char('y') | KeyCode::Enter => self.send_action(a),
                KeyCode::Char('n') | KeyCode::Esc => {}
                _ => self.modal = Some(Modal::Confirm { sure: Sure::Server(a), text, everyone: None }),
            }
            return;
        }
        let Some(repo) = self.local.as_ref().map(|l| l.repo.clone()) else { return };
        match modal {
            Modal::Input { ask, mut value } => match k.code {
                KeyCode::Esc => {}
                KeyCode::Enter if !value.trim().is_empty() => self.submit(ask, value.trim().to_string(), repo),
                KeyCode::Backspace => {
                    value.pop();
                    self.modal = Some(Modal::Input { ask, value });
                }
                KeyCode::Char(c) => {
                    value.push(c);
                    self.modal = Some(Modal::Input { ask, value });
                }
                _ => self.modal = Some(Modal::Input { ask, value }),
            },
            Modal::Confirm { sure, text, everyone } => match k.code {
                KeyCode::Char('y') | KeyCode::Enter => self.confirm(sure, everyone.unwrap_or(false), repo),
                KeyCode::Char(' ') | KeyCode::Char('a') if everyone.is_some() => {
                    self.modal = Some(Modal::Confirm { sure, text, everyone: everyone.map(|e| !e) })
                }
                KeyCode::Char('n') | KeyCode::Esc => {}
                _ => self.modal = Some(Modal::Confirm { sure, text, everyone }),
            },
        }
    }

    fn submit(&mut self, ask: Ask, value: String, repo: Repo) {
        match ask {
            Ask::Save => self.spawn("Saving", move |_| async move {
                let id = repo.commit(&value, &author()).await?;
                Ok(format!("Saved version {}", short(&id)))
            }),
            Ask::Branch => self.spawn("Creating branch", move |_| async move {
                repo.create_branch(&value, None)?;
                Ok(format!("Created branch {value}. Enter switches to it."))
            }),
            Ask::Publish => self.spawn("Publishing", move |_| async move {
                let hub = hub_for(&repo)?;
                let rel = ops::publish(&repo, &hub, &value, "").await?;
                let code = repo.config()?.remote.map(|r| r.code).unwrap_or_default();
                Ok(format!("Published {}. Share code {code}", rel.name))
            }),
        }
    }

    fn confirm(&mut self, sure: Sure, everyone: bool, repo: Repo) {
        match sure {
            Sure::Server(a) => self.send_action(a),
            Sure::Discard => self.spawn("Discarding", move |tx| async move {
                repo.discard(None, &move |p| drop(tx.send(Msg::Progress(p)))).await?;
                Ok("Unsaved changes discarded".into())
            }),
            Sure::Rollback { id } => self.spawn("Rolling back", move |tx| async move {
                let hub = if everyone { Some(hub_for(&repo)?) } else { None };
                let r = ops::rollback(&repo, hub.as_ref(), &id, &author(), &move |p| drop(tx.send(Msg::Progress(p)))).await?;
                Ok(match r.release {
                    Some(rel) => format!("Rolled back. Published {} and deploying to {} server(s)", rel.name, r.servers),
                    None => format!("Rolled back, saved as {}", short(&r.commit)),
                })
            }),
            Sure::Deploy { release } => {
                self.tab = Tab::Releases;
                self.spawn("Deploying", move |_| async move {
                    let hub = hub_for(&repo)?;
                    let pack = repo.config()?.remote.context("publish the pack first")?.pack;
                    let req = DeployRequest { release: release.id.clone(), servers: DeployTargets::All(AllServers::All) };
                    let d = hub.deploy(&pack, &req).await?;
                    Ok(format!("Deploying {} to {} server(s)", release.name, d.servers.len()))
                })
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let [head, body, foot] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(3), Constraint::Length(2)]).areas(f.area());
        f.render_widget(Line::from(vec![" BeaconSync".bold(), "  ".into(), self.account.clone().dark_gray()]), head);
        let [left, right] = Layout::horizontal([Constraint::Length(30), Constraint::Min(20)]).areas(body);
        if !self.server_only {
            self.draw_packs(f, left);
        }
        match &mut self.server {
            Some(scr) if self.server_only => scr.draw(f, body.inner(ratatui::layout::Margin::new(1, 0))),
            Some(scr) => scr.draw(f, right.inner(ratatui::layout::Margin::new(1, 0))),
            None => self.draw_pack(f, right),
        }
        self.draw_footer(f, foot);
        if let Some(m) = &self.modal {
            draw_modal(f, m);
        }
    }

    fn draw_packs(&mut self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .packs
            .iter()
            .map(|p| {
                ListItem::new(vec![
                    Line::from(p.name.clone()),
                    Line::from(format!("{} {}", p.loader.kind.label(), p.minecraft)).dark_gray(),
                ])
            })
            .collect();
        let empty = items.is_empty();
        let list = List::new(items)
            .block(
                Block::new()
                    .borders(Borders::RIGHT)
                    .border_style(Color::DarkGray)
                    .title(" Packs".bold())
                    .padding(ratatui::widgets::Padding::left(1)),
            )
            .highlight_style(highlight(!self.inside));
        f.render_stateful_widget(list, area, &mut self.pack);
        if empty {
            let hint = Paragraph::new("No packs yet.\nbeacon new, import or join\ncreates one.").dark_gray();
            f.render_widget(hint, Rect { y: area.y + 2, height: 3, ..area }.inner(ratatui::layout::Margin::new(1, 0)));
        }
    }

    fn draw_pack(&mut self, f: &mut Frame, area: Rect) {
        let area = area.inner(ratatui::layout::Margin::new(1, 0));
        let [title, tabs, list] =
            Layout::vertical([Constraint::Length(2), Constraint::Length(2), Constraint::Min(1)]).areas(area);
        let Some(l) = &self.local else {
            f.render_widget(Paragraph::new(if self.packs.is_empty() { "" } else { "Loading..." }).dark_gray(), title);
            return;
        };
        let at = match (&l.status.branch, &l.status.head) {
            (Some(b), _) => format!("on {b}"),
            (None, Some(h)) => format!("at {}", short(h)),
            _ => String::new(),
        };
        let role = match (&l.cfg.role, &l.cfg.remote) {
            (Role::Owner, Some(r)) => format!("published, code {}", r.code),
            (Role::Subscriber, _) => "joined".into(),
            _ => "local only".into(),
        };
        let tree = &l.status.diff;
        let head = l.log.first();
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![l.cfg.name.clone().bold(), "  ".into(), format!("{at} · {role}").dark_gray()]),
                Line::from(match head {
                    Some(h) => format!(
                        "{} · {} unsaved",
                        release_of(&self.remote, &h.id).map_or_else(|| short(&h.id).to_string(), str::to_string),
                        tree.files.len()
                    ),
                    None => "nothing saved yet".into(),
                })
                .dark_gray(),
            ]),
            title,
        );
        let tab_line: Vec<Span> = TABS
            .iter()
            .enumerate()
            .flat_map(|(i, t)| {
                let s = format!("{} {}", i + 1, t.label());
                let span = if *t == self.tab { s.white().bold().underlined() } else { s.dark_gray() };
                [span, "   ".into()]
            })
            .collect();
        f.render_widget(Line::from(tab_line), tabs);

        let items: Vec<ListItem> = match self.tab {
            Tab::Changes if tree.is_empty() => vec![ListItem::new("No unsaved changes").dark_gray()],
            Tab::Changes => {
                let mut v: Vec<ListItem> = vec![];
                if let Some((a, b)) = &tree.minecraft {
                    v.push(ListItem::new(Line::from(vec![mark(ChangeKind::Modified), format!("Minecraft {a} -> {b}").into()])));
                }
                for c in &tree.files {
                    let name = c.new.as_ref().or(c.old.as_ref()).and_then(|f| f.entry.name.clone());
                    let version = c.new.as_ref().and_then(|f| f.entry.version.clone()).unwrap_or_default();
                    v.push(ListItem::new(Line::from(vec![
                        mark(c.kind),
                        name.unwrap_or_else(|| c.path.clone()).into(),
                        format!("  {version}  {}", c.path).dark_gray(),
                    ])));
                }
                v
            }
            Tab::History => l
                .log
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let tag = release_of(&self.remote, &e.id).map(|r| format!("{r} ")).unwrap_or_default();
                    ListItem::new(Line::from(vec![
                        if i == 0 { "● ".white() } else { "○ ".dark_gray() },
                        short(&e.id).to_string().bold(),
                        " ".into(),
                        tag.green().bold(),
                        " ".into(),
                        e.commit.message.clone().into(),
                        format!("  {} · {}", e.commit.author, crate::ui::ago(e.commit.time)).dark_gray(),
                    ]))
                })
                .collect(),
            Tab::Branches => l
                .branches
                .iter()
                .map(|b| {
                    ListItem::new(Line::from(vec![
                        if b.current { "* ".green() } else { "  ".into() },
                        b.name.clone().into(),
                        format!("  {}", short(&b.commit)).dark_gray(),
                    ]))
                })
                .collect(),
            Tab::Releases => return self.draw_releases(f, list),
            Tab::Servers => return self.draw_servers(f, list),
        };
        let list_widget = List::new(items).highlight_style(highlight(self.inside));
        f.render_stateful_widget(list_widget, list, &mut self.rows[self.tab as usize]);
    }

    fn draw_releases(&mut self, f: &mut Frame, area: Rect) {
        let r = match &self.remote {
            None if self.local.as_ref().is_some_and(|l| l.cfg.remote.is_none()) => {
                return f.render_widget(Paragraph::new("Not published. Press p to publish a release.").dark_gray(), area);
            }
            None => return f.render_widget(Paragraph::new("Loading...").dark_gray(), area),
            Some(Err(e)) => return f.render_widget(Paragraph::new(e.clone()).red(), area),
            Some(Ok(r)) => r,
        };
        let name_of = |c: &Option<String>| {
            c.as_deref().map(|c| release_of(&self.remote, c).map_or_else(|| short(c).to_string(), str::to_string))
        };
        // Releases on top, servers right under them; the release list scrolls when long.
        let servers_h = r.servers.len().max(1) as u16 + 1;
        let releases_h = (r.releases.len() as u16 + 1).min(area.height.saturating_sub(servers_h).max(2));
        let [rel_area, srv_area] = Layout::vertical([Constraint::Length(releases_h), Constraint::Min(servers_h)]).areas(area);
        let releases: Vec<ListItem> = r
            .releases
            .iter()
            .map(|x| {
                ListItem::new(Line::from(vec![
                    x.name.clone().bold(),
                    format!("  {}  {}", short(&x.commit), crate::ui::ago(x.created)).dark_gray(),
                    if x.notes.is_empty() { "".into() } else { format!("  {}", x.notes).into() },
                ]))
            })
            .collect();
        let list = List::new(releases).highlight_style(highlight(self.inside));
        f.render_stateful_widget(list, rel_area, &mut self.rows[3]);

        let mut lines = vec![Line::from("Servers".bold())];
        if r.servers.is_empty() {
            lines.push(Line::from("None yet. Install one with beacon server install.").dark_gray());
        }
        for s in &r.servers {
            let (dot, state) = match (&s.target, s.online) {
                (Some(_), _) => ("◐ ".yellow(), format!("updating to {}", name_of(&s.target).unwrap_or_default()).yellow()),
                (None, true) => ("● ".green(), name_of(&s.commit).unwrap_or_else(|| "nothing deployed".into()).into()),
                (None, false) => ("○ ".dark_gray(), "offline".dark_gray()),
            };
            lines.push(Line::from(vec![
                dot,
                format!("{:<20} ", s.name).into(),
                format!("{:<22}", s.address).dark_gray(),
                state,
                format!("  {}", s.message).red(),
            ]));
        }
        f.render_widget(Paragraph::new(lines), srv_area);
    }

    fn draw_servers(&mut self, f: &mut Frame, area: Rect) {
        let r = match &self.remote {
            None if self.local.as_ref().is_some_and(|l| l.cfg.remote.is_none()) => {
                return f
                    .render_widget(Paragraph::new("Not published. Publish a release, then install a server.").dark_gray(), area);
            }
            None => return f.render_widget(Paragraph::new("Loading...").dark_gray(), area),
            Some(Err(e)) => return f.render_widget(Paragraph::new(e.clone()).red(), area),
            Some(Ok(r)) => r,
        };
        if r.servers.is_empty() {
            let text = "No servers yet. On the server machine run:\n  beacon login\n  beacon server install";
            return f.render_widget(Paragraph::new(text).dark_gray(), area);
        }
        let name_of = |c: &Option<String>| {
            c.as_deref()
                .map(|c| release_of(&self.remote, c).map_or_else(|| short(c).to_string(), str::to_string))
                .unwrap_or_default()
        };
        let items: Vec<ListItem> = r
            .servers
            .iter()
            .map(|s| {
                let (dot, state) = match (&s.target, s.online, s.owned.as_ref().map(|o| o.status.as_str())) {
                    (Some(_), ..) => ("◐ ".yellow(), format!("updating to {}", name_of(&s.target)).yellow()),
                    (None, true, Some("stopped")) => ("● ".yellow(), "stopped".yellow()),
                    (None, true, _) => ("● ".green(), "online".into()),
                    (None, false, _) => ("○ ".dark_gray(), "offline".dark_gray()),
                };
                ListItem::new(Line::from(vec![
                    dot,
                    format!("{:<20} ", s.name).into(),
                    format!("{:<22}", s.address).dark_gray(),
                    format!("{:<16}", name_of(&s.commit)).into(),
                    state,
                    format!("  {}", s.message).red(),
                ]))
            })
            .collect();
        f.render_stateful_widget(List::new(items).highlight_style(highlight(self.inside)), area, &mut self.rows[4]);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        let [status, keys] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        match (&self.busy, &self.note) {
            (Some((label, Some(p))), _) if p.total > 0 => {
                let ratio = (p.done as f64 / p.total as f64).clamp(0.0, 1.0);
                let g = Gauge::default()
                    .ratio(ratio)
                    .label(format!("{label} {}/{} {}", p.done, p.total, p.label))
                    .gauge_style(Style::new().fg(Color::White).bg(Color::DarkGray));
                f.render_widget(g, status);
            }
            (Some((label, _)), _) => f.render_widget(Line::from(format!(" {label}...")).yellow(), status),
            (None, Some((true, s))) => f.render_widget(Line::from(vec![" ✓ ".green().bold(), s.clone().into()]), status),
            (None, Some((false, s))) => f.render_widget(Line::from(vec![" ! ".yellow().bold(), s.clone().into()]), status),
            (None, None) => {}
        }
        if let Some(scr) = &self.server {
            let spans: Vec<Span> =
                scr.hints().into_iter().flat_map(|(k, v)| [format!(" {k} ").bold(), format!("{v}  ").dark_gray()]).collect();
            return f.render_widget(Line::from(spans), keys);
        }
        let role = self.local.as_ref().map(|l| l.cfg.role);
        let mut hints: Vec<(&str, &str)> = if self.inside { vec![("esc", "packs")] } else { vec![("enter", "open")] };
        hints.extend([("tab", "view")]);
        match self.tab {
            Tab::Changes => hints.extend([("s", "save"), ("x", "discard")]),
            Tab::History if role != Some(Role::Subscriber) => hints.extend([("r", "roll back")]),
            Tab::History => {}
            Tab::Branches => hints.extend([("enter", "switch"), ("n", "new")]),
            Tab::Releases if role == Some(Role::Owner) => hints.extend([("p", "publish"), ("d", "deploy")]),
            Tab::Releases => {}
            Tab::Servers if role == Some(Role::Owner) => hints.extend([("enter", "console and backups")]),
            Tab::Servers => {}
        }
        if role == Some(Role::Subscriber) {
            hints.push(("u", "update"));
        }
        hints.extend([("P", "play"), ("q", "quit")]);
        let spans: Vec<Span> =
            hints.into_iter().flat_map(|(k, v)| [format!(" {k} ").bold(), format!("{v}  ").dark_gray()]).collect();
        f.render_widget(Line::from(spans), keys);
    }
}

fn draw_modal(f: &mut Frame, m: &Modal) {
    let area = f.area();
    let w = area.width.saturating_sub(4).min(64);
    let (title, lines, h) = match m {
        Modal::Input { ask, value } => {
            let title = match ask {
                Ask::Save => " Save a version ",
                Ask::Publish => " Publish a release ",
                Ask::Branch => " New branch ",
            };
            let hint = match ask {
                Ask::Save => "What changed?",
                Ask::Publish => "Release name, e.g. 1.4",
                Ask::Branch => "Branch name",
            };
            let lines = vec![
                Line::from(hint).dark_gray(),
                Line::from(vec![value.clone().into(), " ".reversed()]),
                Line::default(),
                Line::from(vec![" enter ".bold(), "ok  ".dark_gray(), " esc ".bold(), "cancel".dark_gray()]),
            ];
            (title, lines, 6)
        }
        Modal::Confirm { text, everyone, .. } => {
            let mut lines = vec![Line::from(text.clone())];
            if let Some(e) = everyone {
                lines.push(Line::default());
                lines.push(Line::from(vec![
                    if *e { "[x] ".green().bold() } else { "[ ] ".into() },
                    "Also roll back players and servers".into(),
                    "  space".dark_gray(),
                ]));
            }
            lines.push(Line::default());
            lines.push(Line::from(vec![" y ".bold(), "yes  ".dark_gray(), " n ".bold(), "no".dark_gray()]));
            let h = lines.len() as u16 + 2;
            (" Confirm ", lines, h)
        }
    };
    let rect = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h.min(area.height),
    };
    f.render_widget(Clear, rect);
    let block = Block::bordered().title(title.bold()).border_style(Color::White);
    f.render_widget(Paragraph::new(lines).block(block).wrap(ratatui::widgets::Wrap { trim: false }), rect);
}

fn mark(k: ChangeKind) -> Span<'static> {
    match k {
        ChangeKind::Added => "+ ".green(),
        ChangeKind::Removed => "- ".red(),
        ChangeKind::Modified => "~ ".yellow(),
        ChangeKind::Enabled => "● ".green(),
        ChangeKind::Disabled => "○ ".dark_gray(),
    }
}

/// Row highlight: a gray bar where keys go, bold elsewhere so the choice stays visible.
fn highlight(focused: bool) -> Style {
    if focused { Style::new().bg(Color::Indexed(237)) } else { Style::new().add_modifier(Modifier::BOLD) }
}

/// Moves the selection by `by` rows, keeping it inside `len`.
fn step(s: &mut ListState, len: usize, by: isize) {
    if len == 0 {
        return;
    }
    let cur = s.selected().map_or(if by > 0 { -1 } else { 0 }, |i| i as isize);
    s.select(Some((cur + by).clamp(0, len as isize - 1) as usize));
}

fn clamp(s: &mut ListState, len: usize) {
    match s.selected() {
        _ if len == 0 => s.select(None),
        Some(i) if i >= len => s.select(Some(len - 1)),
        None => s.select(Some(0)),
        _ => {}
    }
}
