//! `tess` on its own: the same sending and fetching, in a full-screen view for
//! finding files without typing their paths.
//!
//! Everything runs on the task that reads the keys: the view draws, then
//! waits on whichever comes first of a key, the transfer, a request about the
//! account's shares, or a tick while something moves on its own.

mod accent;
mod app;
mod draw;
mod files;
mod find;
mod input;
mod keys;
mod qr;
mod select;

use std::{
    io::{Write as _, stdout},
    path::PathBuf,
    time::Duration,
};

use crossterm::{
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        EventStream,
    },
    execute,
};
use futures_util::StreamExt as _;
use ratatui_image::picker::{Picker, cap_parser::QueryStdioOptions};

use crate::{Result, client::Client, config};

/// What the view starts with.
#[derive(Debug)]
pub struct Launch {
    /// The folder to start in.
    pub dir: PathBuf,
    /// The instance, when there is one to send to.
    pub client: Option<Client>,
    /// The `[ui]` table of the config file.
    pub settings: config::Ui,
    /// This machine's offset from UTC, read before any thread started.
    pub local_offset: time::UtcOffset,
}

/// How the terminal draws pictures, if it says it can.
///
/// Not asked inside tmux or screen: they pass the question on only when told
/// to, so it would go unanswered.
fn pictures() -> Option<Picker> {
    let term = std::env::var("TERM").unwrap_or_default();
    if std::env::var_os("TMUX").is_some() || term.starts_with("screen") || term.starts_with("tmux")
    {
        return None;
    }
    let options = QueryStdioOptions {
        timeout: Duration::from_secs(1),
        // A QR code is flat colour, and over SSH every byte counts.
        kitty_compression: true,
        ..QueryStdioOptions::default()
    };
    Picker::from_query_stdio_with_options(options)
        .ok()
        .or_else(|| {
            // Its reader still waits on the keys for an answer that ends with
            // the reply to a status request. Asking for one again, plainly,
            // lets it finish instead of keeping the keys pressed meanwhile.
            let mut out = stdout();
            out.write_all(b"\x1b[5n").and_then(|()| out.flush()).ok();
            None
        })
}

/// Runs the view until it is quit.
///
/// # Errors
///
/// When `[ui.bindings]` does not read, or the terminal cannot be drawn on.
pub async fn run(launch: Launch) -> Result<()> {
    let keymap = keys::Keymap::new(launch.settings.keys, &launch.settings.bindings)?;
    let accent = match &launch.settings.accent {
        Some(name) => {
            accent::check(name)?;
            accent::named(name, crate::logo::truecolor())
        }
        None => None,
    };
    let browser = files::Browser::new(
        launch.dir,
        launch.settings.show_hidden,
        launch.settings.sort,
    );
    let mut app = app::App::new(
        keymap,
        launch.settings.mouse,
        browser,
        launch.client,
        accent,
    );
    app.ui = launch.settings;
    app.local_offset = launch.local_offset;

    let (mut terminal, _screen) = Screen::enter(app.mouse)?;
    // Asked before anything else reads the terminal's answers.
    app.picker = pictures();
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
            (id, done) = app::any_done(&mut app.transfers) => Step::Done(id, done),
            Some(reply) = app.requests.next(), if !app.requests.is_empty() => Step::Reply(reply),
            reply = settle(app.signing.as_mut()) => Step::Signing(reply),
            _ = ticks.tick(), if busy => Step::Tick,
        };
        match step {
            Step::Event(Some(event)) => app.handle(event?),
            Step::Event(None) => break,
            Step::Done(id, done) => app.finish(id, done),
            Step::Reply(reply) => app.reply(reply),
            Step::Signing(reply) => {
                app.signing = None;
                app.signing_reply(reply);
            }
            Step::Tick => {}
        }
    }
    Ok(())
}

enum Step {
    Event(Option<std::io::Result<crossterm::event::Event>>),
    Done(u64, app::Done),
    Reply(app::Reply),
    Signing(app::Reply),
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
