//! What the view holds, and what each key, click and paste does to it.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
    time::{Duration, Instant},
};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
};

use super::{
    Defaults,
    files::{Browser, Row},
    find::Finder,
    input::Input,
    keys::{Action, Key, Keymap},
    select::Selection,
};
use crate::{
    Error, Result,
    client::{Client, OwnShare},
    get::{self, Fetched},
    link, output, resume,
    send::{self, Content, Options, Sent},
};

/// Work the view waits on without blocking: polled in the same loop as the
/// keys, so it needs to be neither `Send` nor `'static` beyond what it owns.
pub(crate) type Job<T> = Pin<Box<dyn Future<Output = T>>>;

/// Two clicks on the same thing this close together are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How long a message under the list stays.
const STATUS_FOR: Duration = Duration::from_secs(5);
/// Rows a turn of the mouse wheel moves.
const WHEEL: isize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Files,
    Shares,
}

/// How far a transfer has got.
#[derive(Debug, Default)]
pub(crate) struct Progress {
    /// The file being moved.
    pub(crate) name: String,
    pub(crate) done: u64,
    pub(crate) total: u64,
    /// Bytes of the files already done.
    finished: u64,
    /// Bytes in all, when known before starting.
    pub(crate) all: Option<u64>,
}

impl Progress {
    fn update(&mut self, name: &str, done: u64, total: u64) {
        if name != self.name {
            if !self.name.is_empty() {
                self.finished = self.finished.saturating_add(self.total);
            }
            name.clone_into(&mut self.name);
        }
        self.done = done;
        self.total = total;
    }

    pub(crate) const fn overall(&self) -> u64 {
        self.finished.saturating_add(self.done)
    }
}

/// A send or a download under way.
pub(crate) struct Transfer {
    /// What it is doing, for the dialog's title.
    pub(crate) title: String,
    pub(crate) progress: Rc<RefCell<Progress>>,
    pub(crate) started: Instant,
    /// For a send, what it sends, and whether that is what was marked.
    sending: Option<(String, bool)>,
    job: Job<Done>,
}

impl Transfer {
    pub(crate) const fn job(&mut self) -> &mut Job<Done> {
        &mut self.job
    }
}

impl std::fmt::Debug for Transfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transfer")
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

/// How a transfer ended.
pub(crate) enum Done {
    Sent(Result<Sent>),
    Got {
        result: Result<Fetched>,
        link: String,
        dir: PathBuf,
    },
}

/// How a request about the account's shares ended.
pub(crate) enum Reply {
    Shares(Result<Vec<OwnShare>>),
    Deleted { code: String, result: Result<()> },
}

/// The account's shares, as last fetched.
#[derive(Debug, Default)]
pub(crate) struct SharesView {
    pub(crate) list: Option<std::result::Result<Vec<OwnShare>, String>>,
    pub(crate) cursor: usize,
    pub(crate) loading: bool,
    /// Something changed them since; fetch again when looked at.
    stale: bool,
}

impl SharesView {
    pub(crate) fn current(&self) -> Option<&OwnShare> {
        match &self.list {
            Some(Ok(list)) => list.get(self.cursor),
            _ => None,
        }
    }

    const fn len(&self) -> usize {
        match &self.list {
            Some(Ok(list)) => list.len(),
            _ => 0,
        }
    }

    fn move_by(&mut self, by: isize) {
        let last = self.len().saturating_sub(1);
        self.cursor = self.cursor.saturating_add_signed(by).min(last);
    }
}

/// A field of the send form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Encrypt,
    Password,
    Expires,
    Uses,
    Send,
    Cancel,
}

impl Field {
    const ORDER: [Self; 6] = [
        Self::Encrypt,
        Self::Password,
        Self::Expires,
        Self::Uses,
        Self::Send,
        Self::Cancel,
    ];

    fn step(self, by: isize) -> Self {
        let at = Self::ORDER
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        let count = Self::ORDER.len();
        let next = at
            .saturating_add(count)
            .saturating_add_signed(by)
            .checked_rem(count)
            .unwrap_or(0);
        Self::ORDER.get(next).copied().unwrap_or(self)
    }
}

/// How a share should be made, before it is.
#[derive(Debug)]
pub(crate) struct Form {
    pub(crate) paths: Vec<PathBuf>,
    /// The paths are what was marked, to unmark once they are sent.
    from_marks: bool,
    /// What is being sent, in a line: `3 files · 12.4 MB`.
    pub(crate) summary: String,
    total: u64,
    pub(crate) encrypt: bool,
    pub(crate) password: Input,
    pub(crate) expiries: Vec<(Option<u64>, String)>,
    pub(crate) expiry: usize,
    pub(crate) uses: Input,
    pub(crate) focus: Field,
    pub(crate) error: Option<String>,
}

/// A question with a line to type in.
#[derive(Debug)]
pub(crate) struct Prompt {
    pub(crate) ask: Ask,
    pub(crate) input: Input,
    pub(crate) error: Option<String>,
}

#[derive(Debug)]
pub(crate) enum Ask {
    GoTo,
    Link,
    Password { link: String },
}

#[derive(Debug)]
pub(crate) enum Confirm {
    Delete { code: String, name: String },
    Stop { then_quit: bool },
}

/// What a menu item does.
#[derive(Debug, Clone)]
pub(crate) enum Command {
    Open(Row),
    Send(Row),
    Mark(Row),
    SendMarked,
    CopyText(String),
    CopyShare(String),
    DeleteShare { code: String, name: String },
    GetLink,
    GoTo,
    ToggleHidden,
}

#[derive(Debug)]
pub(crate) struct Menu {
    pub(crate) at: Position,
    pub(crate) items: Vec<(String, Command)>,
    pub(crate) cursor: usize,
}

/// What sits over the list, taking the keys, until it is closed.
#[derive(Debug)]
pub(crate) enum Modal {
    Send(Form),
    Transfer,
    Sent {
        link: String,
        what: String,
        skipped: usize,
        qr: bool,
    },
    Got {
        dir: PathBuf,
        files: Vec<PathBuf>,
    },
    Note(String),
    Failed {
        title: String,
        message: String,
    },
    Prompt(Prompt),
    Confirm(Confirm),
    Menu(Menu),
    Help {
        scroll: usize,
    },
}

/// Buttons, wherever they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    Field(Field),
    ExpiresPrevious,
    ExpiresNext,
    CopyLink,
    ToggleQr,
    Close,
    Stop,
    Yes,
    No,
    Ok,
    MenuItem(usize),
    Footer(Action),
}

/// What is under a point of the screen, recorded as it is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hit {
    Tab(Tab),
    Crumb(PathBuf),
    FileRow(usize),
    FileMark(usize),
    FoundRow(usize),
    FoundMark(usize),
    ShareRow(usize),
    Button(Button),
    /// Inside a dialog, on nothing in particular.
    Dialog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Info,
    Done,
    Problem,
}

/// Everything on screen.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches, each read on its own: not a state machine in disguise"
)]
pub(crate) struct App {
    pub(crate) keymap: Keymap,
    pub(crate) mouse: bool,
    /// The mouse was switched, and the terminal has not been told yet.
    pub(crate) mouse_changed: bool,
    pub(crate) tab: Tab,
    pub(crate) browser: Browser,
    pub(crate) finder: Option<Finder>,
    /// Typing goes to the filter: after `/` with the vim keys.
    pub(crate) filtering: bool,
    pub(crate) modal: Option<Modal>,
    pub(crate) shares: SharesView,
    pub(crate) client: Option<Client>,
    defaults: Defaults,
    pub(crate) transfer: Option<Transfer>,
    pub(crate) request: Option<Job<Reply>>,
    pub(crate) status: Option<(String, Tone, Instant)>,
    /// Where things were drawn, last drawn on top.
    pub(crate) hits: Vec<(Rect, Hit)>,
    pub(crate) selection: Option<Selection>,
    press: Option<Position>,
    last_click: Option<(Hit, Instant)>,
    /// The screen as last drawn, for copying text off it.
    pub(crate) screen: Option<Buffer>,
    /// Rows of the list as last drawn, for paging.
    pub(crate) page: usize,
    /// The first row of each list on screen.
    pub(crate) offsets: Offsets,
    pub(crate) quit: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Offsets {
    pub(crate) files: usize,
    pub(crate) found: usize,
    pub(crate) shares: usize,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("tab", &self.tab)
            .field("dir", &self.browser.dir())
            .finish_non_exhaustive()
    }
}

impl App {
    pub(crate) fn new(
        keymap: Keymap,
        mouse: bool,
        browser: Browser,
        client: Option<Client>,
        defaults: Defaults,
    ) -> Self {
        Self {
            keymap,
            mouse,
            mouse_changed: false,
            tab: Tab::Files,
            browser,
            finder: None,
            filtering: false,
            modal: None,
            shares: SharesView {
                stale: true,
                ..SharesView::default()
            },
            client,
            defaults,
            transfer: None,
            request: None,
            status: None,
            hits: Vec::new(),
            selection: None,
            press: None,
            last_click: None,
            screen: None,
            page: 10,
            offsets: Offsets::default(),
            quit: false,
        }
    }

    /// Whether something on screen moves by itself, so the view should redraw
    /// without waiting for a key.
    pub(crate) fn busy(&self) -> bool {
        self.transfer.is_some()
            || self.request.is_some()
            || self.finder.as_ref().is_some_and(Finder::walking)
            || self.status.is_some()
    }

    /// Catches up with what happened off screen.
    pub(crate) fn tick(&mut self) {
        if let Some(finder) = &mut self.finder {
            finder.poll();
        }
        if self
            .status
            .as_ref()
            .is_some_and(|(_, _, at)| at.elapsed() > STATUS_FOR)
        {
            self.status = None;
        }
    }

    fn say(&mut self, message: impl Into<String>, tone: Tone) {
        self.status = Some((message.into(), tone, Instant::now()));
    }

    pub(crate) fn handle(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key),
            Event::Mouse(mouse) => self.mouse_event(mouse),
            Event::Paste(text) => self.paste(&text),
            _ => {}
        }
    }

    // --- keys ---

    fn key(&mut self, event: KeyEvent) {
        let key = Key::of(&event);
        let action = self.keymap.action(key);
        // Ctrl-C leaves from anywhere, even a dialog with a field in it.
        if event.code == KeyCode::Char('c') && event.modifiers.contains(KeyModifiers::CONTROL) {
            self.act(Action::Quit);
            return;
        }
        if self.modal.is_some() {
            self.modal_key(event, key, action);
            return;
        }
        if self.tab == Tab::Files {
            if self.finder.is_some() {
                self.finder_key(event, key, action);
                return;
            }
            if self.filtering {
                self.filter_key(event, key);
                return;
            }
            if self.keymap.type_to_filter {
                if event.code == KeyCode::Backspace && !self.browser.filter().is_empty() {
                    let mut filter = self.browser.filter().to_owned();
                    filter.pop();
                    self.browser.set_filter(filter);
                    return;
                }
                if action.is_none()
                    && let Some(typed) = key.typed()
                {
                    let mut filter = self.browser.filter().to_owned();
                    filter.push(typed);
                    self.browser.set_filter(filter);
                    return;
                }
            }
            if event.code == KeyCode::Esc {
                if !self.browser.filter().is_empty() {
                    self.browser.set_filter(String::new());
                } else if self.browser.marked_count() > 0 {
                    self.browser.clear_marks();
                    self.say("Marks cleared", Tone::Info);
                }
                return;
            }
        }
        if let Some(action) = action {
            self.act(action);
        }
    }

    fn filter_key(&mut self, event: KeyEvent, key: Key) {
        let mut filter = self.browser.filter().to_owned();
        match event.code {
            KeyCode::Esc => {
                self.filtering = false;
                filter.clear();
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                filter.pop();
            }
            KeyCode::Up => return self.browser.move_by(-1),
            KeyCode::Down => return self.browser.move_by(1),
            _ => match key.typed() {
                Some(typed) => filter.push(typed),
                None => return,
            },
        }
        self.browser.set_filter(filter);
    }

    fn finder_key(&mut self, event: KeyEvent, key: Key, action: Option<Action>) {
        let page = self.page_size();
        let Some(finder) = &mut self.finder else {
            return;
        };
        match event.code {
            KeyCode::Esc => {
                self.finder = None;
                return;
            }
            KeyCode::Enter => return self.jump(),
            // As in fzf: Tab marks, so marking stays one key away while typing.
            KeyCode::Tab => {
                if let Some(found) = finder.current().cloned() {
                    let path = finder.absolute(&found);
                    self.browser.toggle_path(&path, found.is_dir);
                    finder.move_by(1);
                }
                return;
            }
            KeyCode::Up => return finder.move_by(-1),
            KeyCode::Down => return finder.move_by(1),
            KeyCode::PageUp => return finder.move_by(-page),
            KeyCode::PageDown => return finder.move_by(page),
            KeyCode::Backspace => {
                let mut query = finder.query().to_owned();
                query.pop();
                return finder.set_query(query);
            }
            _ => {}
        }
        if let Some(typed) = key.typed() {
            let mut query = finder.query().to_owned();
            query.push(typed);
            return finder.set_query(query);
        }
        if let Some(
            action @ (Action::Send
            | Action::Quit
            | Action::Help
            | Action::Get
            | Action::GoTo
            | Action::ToggleHidden
            | Action::ToggleMouse
            | Action::Find),
        ) = action
        {
            self.act(action);
        }
    }

    fn page_size(&self) -> isize {
        isize::try_from(self.page.max(1)).unwrap_or(1)
    }

    /// Does `action` where the view is.
    pub(crate) fn act(&mut self, action: Action) {
        let page = self.page_size();
        let moves = match action {
            Action::Up => Some(-1),
            Action::Down => Some(1),
            Action::PageUp => Some(-page),
            Action::PageDown => Some(page),
            Action::HalfPageUp => Some(-(page / 2).max(1)),
            Action::HalfPageDown => Some((page / 2).max(1)),
            Action::Top => Some(isize::MIN / 2),
            Action::Bottom => Some(isize::MAX / 2),
            _ => None,
        };
        if let Some(by) = moves {
            match (self.tab, &mut self.finder) {
                (Tab::Files, Some(finder)) => finder.move_by(by),
                (Tab::Files, None) => self.browser.move_by(by),
                (Tab::Shares, _) => self.shares.move_by(by),
            }
            return;
        }
        match action {
            Action::Quit => {
                if self.transfer.is_some() {
                    self.modal = Some(Modal::Confirm(Confirm::Stop { then_quit: true }));
                } else {
                    self.quit = true;
                }
            }
            Action::Help => self.modal = Some(Modal::Help { scroll: 0 }),
            Action::ToggleMouse => {
                self.mouse = !self.mouse;
                self.mouse_changed = true;
                self.say(
                    if self.mouse {
                        "Mouse on: drag to select and copy"
                    } else {
                        "Mouse off: the terminal selects and copies"
                    },
                    Tone::Info,
                );
            }
            Action::NextTab | Action::PreviousTab => self.switch(match self.tab {
                Tab::Files => Tab::Shares,
                Tab::Shares => Tab::Files,
            }),
            Action::Get => self.ask(Ask::Link, ""),
            Action::GoTo => {
                let dir = self.browser.dir().display().to_string();
                self.ask(Ask::GoTo, &dir);
            }
            _ if self.tab == Tab::Shares => self.shares_act(action),
            _ => self.files_act(action),
        }
    }

    fn files_act(&mut self, action: Action) {
        match action {
            Action::Open => {
                if let Some(row) = self.browser.current() {
                    self.run(Command::Open(row));
                }
            }
            Action::Parent => {
                self.finder = None;
                self.browser.up();
            }
            // Like Open, except that on a file it does nothing: an arrow key
            // should not send anything.
            Action::Into => match self.browser.current() {
                Some(Row::Parent) => self.browser.up(),
                Some(row) => {
                    if let Some(entry) = self.browser.entry(row).filter(|entry| entry.is_dir) {
                        let path = entry.path.clone();
                        self.browser.enter(path);
                    }
                }
                None => {}
            },
            Action::Mark => {
                if let Some(row) = self.browser.current() {
                    self.browser.toggle_mark(row);
                    self.browser.move_by(1);
                }
            }
            Action::MarkAll => self.browser.toggle_all(),
            Action::Filter => self.filtering = true,
            Action::Find => {
                self.finder = Some(Finder::start(
                    self.browser.dir().to_path_buf(),
                    self.browser.show_hidden(),
                ));
            }
            Action::Send => match self.browser.current() {
                Some(row) if self.browser.marked_count() == 0 => self.run(Command::Send(row)),
                _ => self.run(Command::SendMarked),
            },
            Action::Copy => {
                if let Some(entry) = self
                    .browser
                    .current()
                    .and_then(|row| self.browser.entry(row))
                {
                    let path = entry.path.display().to_string();
                    self.copy(&path);
                }
            }
            Action::Refresh => self.browser.reload(),
            Action::ToggleHidden => {
                self.browser.toggle_hidden();
                if let Some(finder) = &self.finder {
                    let query = finder.query().to_owned();
                    let mut restarted =
                        Finder::start(self.browser.dir().to_path_buf(), self.browser.show_hidden());
                    restarted.set_query(query);
                    self.finder = Some(restarted);
                }
                self.say(
                    if self.browser.show_hidden() {
                        "Showing dotfiles"
                    } else {
                        "Hiding dotfiles"
                    },
                    Tone::Info,
                );
            }
            _ => {}
        }
    }

    fn shares_act(&mut self, action: Action) {
        let current = self
            .shares
            .current()
            .map(|share| (share.code.clone(), crate::shares::name(share)));
        match (action, current) {
            (Action::Open, Some(_)) => {
                let row = self.shares.cursor.saturating_sub(self.offsets.shares) + 3;
                self.share_menu(Position::new(4, u16::try_from(row).unwrap_or(0)));
            }
            (Action::Copy, Some((code, _))) => self.run(Command::CopyShare(code)),
            (Action::Delete, Some((code, name))) => self.run(Command::DeleteShare { code, name }),
            (Action::Refresh, _) => self.load_shares(),
            _ => {}
        }
    }

    fn switch(&mut self, tab: Tab) {
        self.tab = tab;
        if tab == Tab::Shares && self.shares.stale && !self.shares.loading {
            self.load_shares();
        }
    }

    /// Goes to what the search has under its cursor: into a folder, or to a
    /// file's folder with the cursor on it.
    fn jump(&mut self) {
        let Some(finder) = self.finder.take() else {
            return;
        };
        let Some(found) = finder.current() else {
            return;
        };
        let path = finder.absolute(found);
        if found.is_dir {
            self.browser.enter(path);
        } else if let (Some(dir), Some(name)) = (path.parent(), path.file_name()) {
            self.browser.enter(dir.to_path_buf());
            self.browser.select(&name.to_string_lossy());
        }
    }

    pub(crate) fn run(&mut self, command: Command) {
        match command {
            Command::Open(Row::Parent) => self.browser.up(),
            Command::Open(row) => {
                let Some(entry) = self.browser.entry(row) else {
                    return;
                };
                if entry.is_dir {
                    let path = entry.path.clone();
                    self.browser.enter(path);
                } else if self.browser.marked_count() > 0 {
                    self.run(Command::SendMarked);
                } else {
                    self.run(Command::Send(row));
                }
            }
            Command::Send(row) => {
                if let Some(entry) = self.browser.entry(row) {
                    let path = entry.path.clone();
                    self.open_form(vec![path], false);
                }
            }
            Command::Mark(row) => self.browser.toggle_mark(row),
            Command::SendMarked => {
                let paths: Vec<PathBuf> = self
                    .browser
                    .marked()
                    .map(|(path, _)| path.clone())
                    .collect();
                if paths.is_empty() {
                    let mark = self.keymap.hint(Action::Mark).unwrap_or_default();
                    self.say(format!("Nothing marked; {mark} marks a file"), Tone::Info);
                } else {
                    self.open_form(paths, true);
                }
            }
            Command::CopyText(text) => self.copy(&text),
            Command::CopyShare(code) => self.copy_share(&code),
            Command::DeleteShare { code, name } => {
                self.modal = Some(Modal::Confirm(Confirm::Delete { code, name }));
            }
            Command::GetLink => self.ask(Ask::Link, ""),
            Command::GoTo => self.act(Action::GoTo),
            Command::ToggleHidden => self.files_act(Action::ToggleHidden),
        }
    }

    fn ask(&mut self, ask: Ask, text: &str) {
        let input = match ask {
            Ask::Password { .. } => Input::masked(),
            _ => Input::new(text),
        };
        self.modal = Some(Modal::Prompt(Prompt {
            ask,
            input,
            error: None,
        }));
    }

    fn copy(&mut self, text: &str) {
        match output::copy(text) {
            Ok(_) => {
                let count = text.chars().count();
                let shown = if count > 40 {
                    format!("{count} characters")
                } else {
                    format!("“{text}”")
                };
                self.say(format!("Copied {shown}"), Tone::Done);
            }
            Err(error) => self.say(error.to_string(), Tone::Problem),
        }
    }

    fn copy_share(&mut self, code: &str) {
        let Some(client) = &self.client else {
            return;
        };
        let encrypted = self
            .shares
            .current()
            .is_some_and(|share| share.code == code && share.encryption.is_some());
        let link = link::to_share(client.server(), code, None);
        self.copy(&link);
        if encrypted {
            self.say(
                "Copied, without its key: an encrypted share opens only with the link it was \
                 sent with",
                Tone::Info,
            );
        }
    }

    // --- the send form ---

    fn open_form(&mut self, paths: Vec<PathBuf>, from_marks: bool) {
        if self.client.is_none() {
            self.modal = Some(Modal::Failed {
                title: "No instance to send to".to_owned(),
                message: "Run `tess login https://your.instance` first, or start with --server."
                    .to_owned(),
            });
            return;
        }
        let (summary, total) = match send::expand(&paths, !self.defaults.encrypt) {
            Ok((entries, _)) => {
                let total = entries
                    .iter()
                    .filter_map(|entry| std::fs::metadata(&entry.path).ok())
                    .map(|meta| meta.len())
                    .sum::<u64>();
                let what = match entries.as_slice() {
                    [one] => one.name.clone(),
                    many => format!("{} files", many.len()),
                };
                (format!("{what} · {}", crate::style::bytes(total)), total)
            }
            Err(error) => {
                self.modal = Some(Modal::Failed {
                    title: "Cannot send that".to_owned(),
                    message: error.to_string(),
                });
                return;
            }
        };
        let mut expiries = vec![
            (None, "Instance default".to_owned()),
            (Some(1), "1 hour".to_owned()),
            (Some(24), "1 day".to_owned()),
            (Some(72), "3 days".to_owned()),
            (Some(168), "7 days".to_owned()),
            (Some(720), "30 days".to_owned()),
        ];
        let expiry = match self.defaults.expires_in_hours {
            None => 0,
            Some(hours) => expiries
                .iter()
                .position(|(choice, _)| *choice == Some(hours))
                .unwrap_or_else(|| {
                    expiries.push((Some(hours), format!("{hours} hours")));
                    expiries.len() - 1
                }),
        };
        let mut password = Input::masked();
        if let Some(given) = &self.defaults.password {
            password.paste(given);
        }
        self.modal = Some(Modal::Send(Form {
            paths,
            from_marks,
            summary,
            total,
            encrypt: self.defaults.encrypt,
            password,
            expiries,
            expiry,
            uses: Input::new(
                &self
                    .defaults
                    .max_uses
                    .map(|uses| uses.to_string())
                    .unwrap_or_default(),
            ),
            focus: Field::Send,
            error: None,
        }));
    }

    fn form_key(form: &mut Form, event: KeyEvent, key: Key, action: Option<Action>) -> FormOutcome {
        match event.code {
            KeyCode::Esc => return FormOutcome::Cancel,
            KeyCode::Tab | KeyCode::Down => form.focus = form.focus.step(1),
            KeyCode::BackTab | KeyCode::Up => form.focus = form.focus.step(-1),
            // On the switch, Enter flips it, as Space and the arrows do:
            // reaching it with the arrows and pressing Enter should not send.
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right
                if form.focus == Field::Encrypt =>
            {
                form.encrypt = !form.encrypt;
            }
            KeyCode::Enter => {
                return if form.focus == Field::Cancel {
                    FormOutcome::Cancel
                } else {
                    FormOutcome::Submit
                };
            }
            KeyCode::Left if form.focus == Field::Expires => form.cycle(-1),
            KeyCode::Right if form.focus == Field::Expires => form.cycle(1),
            code => match form.focus {
                Field::Password => {
                    if !form.password.edit(code)
                        && let Some(typed) = key.typed()
                    {
                        form.password.insert(typed);
                    }
                }
                Field::Uses => {
                    if !form.uses.edit(code)
                        && let Some(typed) = key.typed().filter(char::is_ascii_digit)
                    {
                        form.uses.insert(typed);
                    }
                }
                _ if action == Some(Action::Send) => return FormOutcome::Submit,
                _ => {}
            },
        }
        FormOutcome::Stay
    }

    fn submit(&mut self, form: Form) {
        let max_uses = if form.uses.text().is_empty() {
            None
        } else {
            match form.uses.text().parse::<u32>() {
                Ok(uses) if uses > 0 => Some(uses),
                _ => {
                    let mut form = form;
                    form.error = Some("Downloads must be a whole number above 0".to_owned());
                    form.focus = Field::Uses;
                    self.modal = Some(Modal::Send(form));
                    return;
                }
            }
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let options = Options {
            encrypt: form.encrypt,
            password: Some(form.password.text().to_owned()).filter(|password| !password.is_empty()),
            expires_in_hours: form.expiries.get(form.expiry).and_then(|(hours, _)| *hours),
            max_uses,
            resume: resume::default_dir().ok(),
        };
        let progress = Rc::new(RefCell::new(Progress {
            all: Some(form.total),
            ..Progress::default()
        }));
        let report = Rc::clone(&progress);
        let paths = form.paths;
        let job: Job<Done> = Box::pin(async move {
            let mut update =
                |name: &str, done: u64, total: u64| report.borrow_mut().update(name, done, total);
            Done::Sent(send::send(&client, Content::Files(paths), options, &mut update).await)
        });
        self.transfer = Some(Transfer {
            title: "Sending".to_owned(),
            progress,
            started: Instant::now(),
            sending: Some((form.summary, form.from_marks)),
            job,
        });
        self.modal = Some(Modal::Transfer);
    }

    // --- downloads ---

    fn start_get(&mut self, text: &str, password: Option<String>) -> Result<()> {
        let server = self.client.as_ref().map(Client::server);
        let target = link::parse(text.trim(), server)?;
        let client = Client::new(target.server.clone(), None)?;
        let dir = self.browser.dir().to_path_buf();
        let progress = Rc::new(RefCell::new(Progress::default()));
        let report = Rc::clone(&progress);
        let link = text.trim().to_owned();
        let code = target.code.clone();
        let job: Job<Done> = Box::pin(async move {
            let mut update = |name: &str, done: u64, total: u64| {
                report.borrow_mut().update(name, done, total);
            };
            let result = get::get(
                &client,
                &target.code,
                target.key,
                password.as_deref(),
                &dir,
                &mut update,
            )
            .await;
            Done::Got { result, link, dir }
        });
        self.transfer = Some(Transfer {
            title: format!("Downloading {code}"),
            progress,
            started: Instant::now(),
            sending: None,
            job,
        });
        self.modal = Some(Modal::Transfer);
        Ok(())
    }

    /// Takes the end of a transfer.
    pub(crate) fn finish(&mut self, done: Done) {
        let (what, from_marks) = self
            .transfer
            .take()
            .and_then(|transfer| transfer.sending)
            .unwrap_or_default();
        match done {
            Done::Sent(Ok(sent)) => {
                if from_marks {
                    self.browser.clear_marks();
                }
                self.shares.stale = true;
                self.modal = Some(Modal::Sent {
                    link: sent.link,
                    what,
                    skipped: sent.skipped_empty,
                    qr: false,
                });
            }
            Done::Sent(Err(error)) => {
                self.modal = Some(Modal::Failed {
                    title: "Not sent".to_owned(),
                    message: format!(
                        "{error}\n\nSending the same files again picks up where this stopped."
                    ),
                });
            }
            Done::Got {
                result: Ok(fetched),
                dir,
                ..
            } => {
                self.browser.reload();
                if let Some(first) = fetched.files.first().and_then(|path| path.file_name()) {
                    self.browser.select(&first.to_string_lossy());
                }
                self.modal = Some(match fetched.note {
                    Some(note) => Modal::Note(String::from_utf8_lossy(&note).into_owned()),
                    None => Modal::Got {
                        dir,
                        files: fetched.files,
                    },
                });
            }
            Done::Got {
                result: Err(Error::PasswordRequired),
                link,
                ..
            } => self.ask(Ask::Password { link }, ""),
            Done::Got {
                result: Err(error), ..
            } => {
                let message = match error {
                    Error::Server { status: 404, .. } => {
                        "No such share; it may have expired or been used up.".to_owned()
                    }
                    Error::Server { status: 401, .. } => "Wrong password.".to_owned(),
                    other => other.to_string(),
                };
                self.modal = Some(Modal::Failed {
                    title: "Not downloaded".to_owned(),
                    message,
                });
            }
        }
    }

    // --- the account's shares ---

    fn load_shares(&mut self) {
        let Some(client) = self.client.clone().filter(Client::signed_in) else {
            self.shares.list = Some(Err(match &self.client {
                Some(client) => format!(
                    "Not signed in to {}. Run `tess login {}` to list your shares here.",
                    client.server(),
                    client.server()
                ),
                None => "No instance. Run `tess login https://your.instance` first.".to_owned(),
            }));
            self.shares.stale = false;
            return;
        };
        self.shares.loading = true;
        self.request = Some(Box::pin(async move {
            Reply::Shares(crate::shares::all_mine(&client).await)
        }));
    }

    pub(crate) fn reply(&mut self, reply: Reply) {
        self.request = None;
        match reply {
            Reply::Shares(result) => {
                self.shares.loading = false;
                self.shares.stale = false;
                self.shares.list = Some(result.map_err(|error| error.to_string()));
                self.shares.move_by(0);
            }
            Reply::Deleted { code, result } => match result {
                Ok(()) => {
                    self.say(format!("Deleted {code}"), Tone::Done);
                    if let Some(Ok(list)) = &mut self.shares.list {
                        list.retain(|share| share.code != code);
                    }
                    self.shares.move_by(0);
                }
                Err(error) => self.say(format!("{code} not deleted: {error}"), Tone::Problem),
            },
        }
    }

    fn delete_share(&mut self, code: String) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.request.is_some() {
            self.say(
                "Still busy with the list; try again in a moment",
                Tone::Info,
            );
            return;
        }
        self.request = Some(Box::pin(async move {
            let result = client.delete_share(&code).await;
            Reply::Deleted { code, result }
        }));
    }

    // --- dialogs ---

    fn modal_key(&mut self, event: KeyEvent, key: Key, action: Option<Action>) {
        let Some(modal) = self.modal.take() else {
            return;
        };
        let close = event.code == KeyCode::Esc;
        match modal {
            Modal::Send(mut form) => match Self::form_key(&mut form, event, key, action) {
                FormOutcome::Stay => self.modal = Some(Modal::Send(form)),
                FormOutcome::Cancel => {}
                FormOutcome::Submit => self.submit(form),
            },
            Modal::Transfer => {
                self.modal = Some(if close || action == Some(Action::Quit) {
                    Modal::Confirm(Confirm::Stop { then_quit: false })
                } else {
                    Modal::Transfer
                });
            }
            Modal::Sent {
                link,
                what,
                skipped,
                mut qr,
            } => {
                if close {
                    return;
                }
                // Enter does what the focused button says: copy.
                if event.code == KeyCode::Enter
                    || action == Some(Action::Copy)
                    || key.typed() == Some('c')
                {
                    self.copy(&link);
                }
                if key.typed() == Some('q') {
                    qr = !qr;
                }
                self.modal = Some(Modal::Sent {
                    link,
                    what,
                    skipped,
                    qr,
                });
            }
            Modal::Got { .. } | Modal::Failed { .. } if close || event.code == KeyCode::Enter => {}
            Modal::Note(text) => match event.code {
                KeyCode::Esc | KeyCode::Enter => {}
                _ => {
                    if action == Some(Action::Copy) || key.typed() == Some('c') {
                        self.copy(&text);
                    }
                    self.modal = Some(Modal::Note(text));
                }
            },
            Modal::Prompt(mut prompt) => match event.code {
                KeyCode::Esc => {}
                KeyCode::Enter => self.answer(prompt),
                code => {
                    if !prompt.input.edit(code)
                        && let Some(typed) = key.typed()
                    {
                        prompt.input.insert(typed);
                    }
                    self.modal = Some(Modal::Prompt(prompt));
                }
            },
            Modal::Confirm(confirm) => match key.typed().or(match event.code {
                KeyCode::Enter => Some('y'),
                KeyCode::Esc => Some('n'),
                _ => None,
            }) {
                Some('y' | 'Y') => self.confirmed(confirm),
                Some('n' | 'N') => {}
                _ => self.modal = Some(Modal::Confirm(confirm)),
            },
            Modal::Menu(menu) => self.menu_key(menu, event.code),
            Modal::Help { scroll } => match (event.code, action) {
                (KeyCode::Esc | KeyCode::Enter, _) | (_, Some(Action::Help)) => {}
                (_, Some(Action::Up)) => {
                    self.modal = Some(Modal::Help {
                        scroll: scroll.saturating_sub(1),
                    });
                }
                (_, Some(Action::Down)) => {
                    self.modal = Some(Modal::Help { scroll: scroll + 1 });
                }
                _ => self.modal = Some(Modal::Help { scroll }),
            },
            other => self.modal = Some(other),
        }
    }

    fn menu_key(&mut self, mut menu: Menu, code: KeyCode) {
        match code {
            KeyCode::Esc => return,
            KeyCode::Up => menu.cursor = menu.cursor.saturating_sub(1),
            KeyCode::Down => {
                menu.cursor = (menu.cursor + 1).min(menu.items.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                if let Some((_, command)) = menu.items.into_iter().nth(menu.cursor) {
                    self.run(command);
                }
                return;
            }
            _ => {}
        }
        self.modal = Some(Modal::Menu(menu));
    }

    fn answer(&mut self, mut prompt: Prompt) {
        let text = prompt.input.text().trim().to_owned();
        let outcome = match &prompt.ask {
            Ask::GoTo => self.go_to(&text),
            Ask::Link => self
                .start_get(&text, None)
                .map_err(|error| error.to_string()),
            Ask::Password { link } => {
                let link = link.clone();
                self.start_get(&link, Some(prompt.input.text().to_owned()))
                    .map_err(|error| error.to_string())
            }
        };
        if let Err(error) = outcome {
            prompt.error = Some(error);
            self.modal = Some(Modal::Prompt(prompt));
        }
    }

    fn go_to(&mut self, text: &str) -> std::result::Result<(), String> {
        let path = match text.strip_prefix('~') {
            Some(rest) => std::env::home_dir()
                .map(|home| home.join(rest.trim_start_matches(['/', '\\'])))
                .ok_or_else(|| "No home folder to start from".to_owned())?,
            None => self.browser.dir().join(text),
        };
        let path = std::fs::canonicalize(&path).map_err(|error| format!("{text}: {error}"))?;
        if path.is_dir() {
            self.finder = None;
            self.browser.enter(path);
        } else if let (Some(dir), Some(name)) = (path.parent(), path.file_name()) {
            self.finder = None;
            self.browser.enter(dir.to_path_buf());
            self.browser.select(&name.to_string_lossy());
        }
        Ok(())
    }

    fn confirmed(&mut self, confirm: Confirm) {
        match confirm {
            Confirm::Delete { code, .. } => self.delete_share(code),
            Confirm::Stop { then_quit } => {
                self.transfer = None;
                self.say(
                    "Stopped. Sending the same files again picks up where this stopped.",
                    Tone::Info,
                );
                self.quit = then_quit;
            }
        }
    }

    // --- the mouse ---

    fn mouse_event(&mut self, event: MouseEvent) {
        let at = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.press = Some(at);
                self.selection = None;
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(anchor) = self.press
                    && (anchor != at || self.selection.is_some())
                {
                    self.selection = Some(Selection { anchor, head: at });
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pressed = self.press.take();
                if let Some(selection) = self.selection.take() {
                    let text = self
                        .screen
                        .as_ref()
                        .map(|screen| selection.text(screen))
                        .unwrap_or_default();
                    if !text.is_empty() {
                        self.copy(&text);
                    }
                } else if let Some(at) = pressed {
                    self.click(at, event.modifiers);
                }
            }
            MouseEventKind::Down(MouseButton::Right) => self.context_menu(at),
            MouseEventKind::ScrollUp => self.wheel(-WHEEL),
            MouseEventKind::ScrollDown => self.wheel(WHEEL),
            _ => {}
        }
    }

    fn hit_at(&self, at: Position) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(area, _)| area.contains(at))
            .map(|(_, hit)| hit.clone())
    }

    fn wheel(&mut self, by: isize) {
        match &mut self.modal {
            Some(Modal::Help { scroll }) => *scroll = scroll.saturating_add_signed(by),
            Some(_) => {}
            None => match (self.tab, &mut self.finder) {
                (Tab::Files, Some(finder)) => finder.move_by(by),
                (Tab::Files, None) => self.browser.move_by(by),
                (Tab::Shares, _) => self.shares.move_by(by),
            },
        }
    }

    fn click(&mut self, at: Position, modifiers: KeyModifiers) {
        let hit = self.hit_at(at);
        // A click off a menu only closes it.
        if matches!(self.modal, Some(Modal::Menu(_)))
            && !matches!(hit, Some(Hit::Button(Button::MenuItem(_))))
        {
            self.modal = None;
            return;
        }
        let Some(hit) = hit else {
            return;
        };
        let double = self
            .last_click
            .as_ref()
            .is_some_and(|(last, when)| *last == hit && when.elapsed() < DOUBLE_CLICK);
        self.last_click = if double {
            None
        } else {
            Some((hit.clone(), Instant::now()))
        };
        if self.modal.is_some() && !matches!(hit, Hit::Button(_) | Hit::Dialog) {
            return;
        }
        let toggle = modifiers.contains(KeyModifiers::CONTROL);
        match hit {
            Hit::Tab(tab) => self.switch(tab),
            Hit::Crumb(path) => {
                self.finder = None;
                self.browser.enter(path);
            }
            Hit::FileRow(row) | Hit::FileMark(row) => {
                self.browser.move_to(row);
                let Some(current) = self.browser.current() else {
                    return;
                };
                if toggle || matches!(hit, Hit::FileMark(_)) {
                    self.browser.toggle_mark(current);
                } else if double {
                    self.run(Command::Open(current));
                }
            }
            Hit::FoundRow(row) | Hit::FoundMark(row) => {
                let Some(finder) = &mut self.finder else {
                    return;
                };
                finder.move_to(row);
                if toggle || matches!(hit, Hit::FoundMark(_)) {
                    if let Some(found) = finder.current().cloned() {
                        let path = finder.absolute(&found);
                        self.browser.toggle_path(&path, found.is_dir);
                    }
                } else if double {
                    self.jump();
                }
            }
            Hit::ShareRow(row) => {
                self.shares.cursor = row;
                self.shares.move_by(0);
                if double {
                    self.share_menu(at);
                }
            }
            Hit::Button(button) => self.button(button),
            Hit::Dialog => {}
        }
    }

    fn button(&mut self, button: Button) {
        match (button, self.modal.take()) {
            (Button::Footer(action), modal) => {
                self.modal = modal;
                self.act(action);
            }
            (Button::Field(field), Some(Modal::Send(mut form))) => {
                match field {
                    Field::Send => return self.submit(form),
                    Field::Cancel => return,
                    Field::Encrypt => form.encrypt = !form.encrypt,
                    _ => {}
                }
                form.focus = field;
                self.modal = Some(Modal::Send(form));
            }
            (Button::ExpiresPrevious | Button::ExpiresNext, Some(Modal::Send(mut form))) => {
                form.focus = Field::Expires;
                form.cycle(if button == Button::ExpiresNext { 1 } else { -1 });
                self.modal = Some(Modal::Send(form));
            }
            (
                Button::CopyLink,
                Some(Modal::Sent {
                    link,
                    what,
                    skipped,
                    qr,
                }),
            ) => {
                self.copy(&link);
                self.modal = Some(Modal::Sent {
                    link,
                    what,
                    skipped,
                    qr,
                });
            }
            (Button::CopyLink, Some(Modal::Note(text))) => {
                self.copy(&text);
                self.modal = Some(Modal::Note(text));
            }
            (
                Button::ToggleQr,
                Some(Modal::Sent {
                    link,
                    what,
                    skipped,
                    qr,
                }),
            ) => {
                self.modal = Some(Modal::Sent {
                    link,
                    what,
                    skipped,
                    qr: !qr,
                });
            }
            (Button::Stop, Some(Modal::Transfer)) => {
                self.modal = Some(Modal::Confirm(Confirm::Stop { then_quit: false }));
            }
            (Button::Yes, Some(Modal::Confirm(confirm))) => self.confirmed(confirm),
            (Button::Ok, Some(Modal::Prompt(prompt))) => self.answer(prompt),
            (Button::MenuItem(index), Some(Modal::Menu(menu))) => {
                if let Some((_, command)) = menu.items.into_iter().nth(index) {
                    self.run(command);
                }
            }
            (Button::Close | Button::No, _) => {}
            (_, modal) => self.modal = modal,
        }
    }

    fn context_menu(&mut self, at: Position) {
        if self.modal.is_some() {
            return;
        }
        let hit = self.hit_at(at);
        if self.tab == Tab::Shares {
            if let Some(Hit::ShareRow(row)) = hit {
                self.shares.cursor = row;
                self.share_menu(at);
            }
            return;
        }
        let mut items = Vec::new();
        if let Some(Hit::FileRow(row) | Hit::FileMark(row)) = hit {
            self.browser.move_to(row);
            if let Some(current) = self.browser.current()
                && let Some(entry) = self.browser.entry(current)
            {
                let marked = self.browser.is_marked(&entry.path);
                if entry.is_dir {
                    items.push(("Open".to_owned(), Command::Open(current)));
                    items.push(("Send this folder".to_owned(), Command::Send(current)));
                } else {
                    items.push(("Send".to_owned(), Command::Send(current)));
                }
                items.push((
                    if marked { "Unmark" } else { "Mark" }.to_owned(),
                    Command::Mark(current),
                ));
                items.push((
                    "Copy path".to_owned(),
                    Command::CopyText(entry.path.display().to_string()),
                ));
            }
        }
        let marked = self.browser.marked_count();
        if marked > 0 {
            items.push((format!("Send marked ({marked})"), Command::SendMarked));
        }
        items.push(("Download a link here".to_owned(), Command::GetLink));
        items.push(("Go to a path".to_owned(), Command::GoTo));
        items.push((
            if self.browser.show_hidden() {
                "Hide dotfiles"
            } else {
                "Show dotfiles"
            }
            .to_owned(),
            Command::ToggleHidden,
        ));
        self.modal = Some(Modal::Menu(Menu {
            at,
            items,
            cursor: 0,
        }));
    }

    fn share_menu(&mut self, at: Position) {
        let Some(share) = self.shares.current() else {
            return;
        };
        let code = share.code.clone();
        let name = crate::shares::name(share);
        self.modal = Some(Modal::Menu(Menu {
            at,
            items: vec![
                ("Copy link".to_owned(), Command::CopyShare(code.clone())),
                ("Delete".to_owned(), Command::DeleteShare { code, name }),
            ],
            cursor: 0,
        }));
    }

    // --- pasting ---

    fn paste(&mut self, text: &str) {
        match &mut self.modal {
            Some(Modal::Prompt(prompt)) => return prompt.input.paste(text),
            Some(Modal::Send(form)) => {
                match form.focus {
                    Field::Password => form.password.paste(text),
                    Field::Uses => form.uses.paste(
                        &text
                            .chars()
                            .filter(char::is_ascii_digit)
                            .collect::<String>(),
                    ),
                    _ => {}
                }
                return;
            }
            Some(_) => return,
            None => {}
        }
        let line = text.lines().next().unwrap_or_default().trim();
        if looks_like_a_link(line) {
            self.ask(Ask::Link, line);
            return;
        }
        if let Some(finder) = &mut self.finder {
            let mut query = finder.query().to_owned();
            query.push_str(line);
            finder.set_query(query);
        } else if self.tab == Tab::Files && (self.keymap.type_to_filter || self.filtering) {
            let mut filter = self.browser.filter().to_owned();
            filter.push_str(line);
            self.browser.set_filter(filter);
        }
    }
}

enum FormOutcome {
    Stay,
    Cancel,
    Submit,
}

impl Form {
    fn cycle(&mut self, by: isize) {
        let count = self.expiries.len().max(1);
        self.expiry = self
            .expiry
            .saturating_add(count)
            .saturating_add_signed(by)
            .checked_rem(count)
            .unwrap_or(0);
    }
}

/// Whether pasted text is a share link, to offer to download it.
fn looks_like_a_link(text: &str) -> bool {
    url::Url::parse(text).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url
                .path_segments()
                .and_then(|mut segments| segments.next())
                .is_some_and(|first| first == "s")
    })
}

/// The path written the way people write it: `~` for the home folder.
pub(crate) fn tilde(path: &Path) -> String {
    std::env::home_dir()
        .and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
        .map_or_else(
            || path.display().to_string(),
            |rest| {
                if rest.as_os_str().is_empty() {
                    "~".to_owned()
                } else {
                    format!("~/{}", rest.display())
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_adds_up_across_files() {
        let mut progress = Progress::default();
        progress.update("a.txt", 0, 10);
        progress.update("a.txt", 10, 10);
        progress.update("b.txt", 4, 20);
        assert_eq!(progress.overall(), 14);
        assert_eq!(progress.name, "b.txt");
    }

    #[test]
    fn a_share_link_is_told_from_other_text() {
        assert!(looks_like_a_link("https://share.example/s/AB12#key"));
        assert!(!looks_like_a_link("https://share.example/admin"));
        assert!(!looks_like_a_link("notes.txt"));
    }

    #[test]
    fn the_form_cycles_through_its_fields() {
        assert_eq!(Field::Cancel.step(1), Field::Encrypt);
        assert_eq!(Field::Encrypt.step(-1), Field::Cancel);
        assert_eq!(Field::Password.step(1), Field::Expires);
    }
}
