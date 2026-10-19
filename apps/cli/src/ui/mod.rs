//! `tess ui`: the same sending and fetching, in a full-screen view for
//! finding files without typing their paths.
//!
//! Everything runs on the task that reads the keys: the view draws, then
//! waits on whichever comes first of a key, the transfer, a request about the
//! account's shares, or a tick while something moves on its own.

mod app;
mod draw;
mod files;
mod find;
mod input;
mod keys;
mod select;

use std::{io::stdout, path::PathBuf, time::Duration};

use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        EventStream,
    },
    execute,
};
use futures_util::StreamExt as _;

use crate::{Result, client::Client, config};

/// How a send starts out, from the flags `tess send` was given.
#[derive(Debug, Clone, Default)]
pub struct Defaults {
    /// Seal before sending.
    pub encrypt: bool,
    /// The share's password, or the one its key comes from.
    pub password: Option<String>,
    /// How many hours it lasts.
    pub expires_in_hours: Option<u64>,
    /// How many times it may be retrieved.
    pub max_uses: Option<u32>,
}

/// What the view starts with.
#[derive(Debug)]
pub struct Launch {
    /// The folder to start in.
    pub dir: PathBuf,
    /// The instance, when there is one to send to.
    pub client: Option<Client>,
    /// How sends start out.
    pub defaults: Defaults,
    /// The `[ui]` table of the config file.
    pub settings: config::Ui,
}

/// Runs the view until it is quit.
///
/// # Errors
///
/// When `[ui.bindings]` does not read, or the terminal cannot be drawn on.
pub async fn run(launch: Launch) -> Result<()> {
    let keymap = keys::Keymap::new(launch.settings.keys, &launch.settings.bindings)?;
    let browser = files::Browser::new(launch.dir, launch.settings.show_hidden);
    let mut app = app::App::new(
        keymap,
        launch.settings.mouse,
        browser,
        launch.client,
        launch.defaults,
    );

    let (mut terminal, _screen) = Screen::enter(app.mouse)?;
    let mut events = EventStream::new();
    let mut ticks = tokio::time::interval(Duration::from_millis(100));
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    while !app.quit {
        app.tick();
        let drawn = terminal.draw(|frame| draw::draw(&mut app, frame))?;
        app.screen = Some(drawn.buffer.clone());
        if app.mouse_changed {
            app.mouse_changed = false;
            if app.mouse {
                execute!(stdout(), EnableMouseCapture)?;
            } else {
                execute!(stdout(), DisableMouseCapture)?;
            }
        }
        let busy = app.busy();
        let step = tokio::select! {
            event = events.next() => Step::Event(event),
            done = settle(app.transfer.as_mut().map(app::Transfer::job)) => Step::Done(done),
            reply = settle(app.request.as_mut()) => Step::Reply(reply),
            _ = ticks.tick(), if busy => Step::Tick,
        };
        match step {
            Step::Event(Some(event)) => app.handle(event?),
            Step::Event(None) => break,
            Step::Done(done) => app.finish(done),
            Step::Reply(reply) => app.reply(reply),
            Step::Tick => {}
        }
    }
    Ok(())
}

enum Step {
    Event(Option<std::io::Result<crossterm::event::Event>>),
    Done(app::Done),
    Reply(app::Reply),
    Tick,
}

/// What `job` comes to, or never, when there is none.
async fn settle<T>(job: Option<&mut app::Job<T>>) -> T {
    match job {
        Some(job) => job.as_mut().await,
        None => std::future::pending().await,
    }
}

/// The terminal, taken over: raw keys, its own screen, pastes as one event,
/// and the mouse when asked. Given back when dropped, a panic included.
struct Screen;

impl Screen {
    fn enter(mouse: bool) -> Result<(ratatui::DefaultTerminal, Self)> {
        let terminal = ratatui::try_init()?;
        let screen = Self;
        execute!(stdout(), EnableBracketedPaste)?;
        if mouse {
            execute!(stdout(), EnableMouseCapture)?;
        }
        Ok((terminal, screen))
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        // Leaving as much of it as will go; nothing else to do if it will not.
        execute!(stdout(), DisableMouseCapture, DisableBracketedPaste).ok();
        ratatui::restore();
    }
}
