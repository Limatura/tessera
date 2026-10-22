//! Drawing the view, and noting where each clickable thing landed.

use std::time::SystemTime;

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthChar as _;

use super::{
    app::{
        App, Ask, Button, Compose, ComposeField, Confirm, Field, Form, Hit, LANGUAGES, Menu, Modal,
        Prompt, SETTINGS, Setting, SignIn, Tab, Tone, Transfer, tilde,
    },
    files::{Marked, Row},
    keys::Action,
    motion,
};
use crate::{
    logo,
    style::{SPINNER, bytes},
};

thread_local! {
    /// The colour that marks what matters: the ticket's orange until the
    /// site's accent is known. Kept here, not passed down, since nearly every
    /// drawing function wants it and the view draws on one thread.
    static ACCENT: std::cell::Cell<Color> =
        std::cell::Cell::new(super::accent::brand(logo::truecolor()));
}

fn accent() -> Color {
    ACCENT.with(std::cell::Cell::get)
}

/// Switches the accent, from the next frame on.
pub(crate) fn set_accent(color: Color) {
    ACCENT.with(|cell| cell.set(color));
}

/// The ticket, a line to a row of cells.
fn ticket(picture: logo::Picture) -> Vec<Line<'static>> {
    motion::still(picture, logo::truecolor())
}

/// The ticket in the middle of `area` with `note` under it, when there is room;
/// otherwise only the note, at the top.
fn draw_ticket_with(frame: &mut Frame<'_>, area: Rect, note: &str) {
    let tall = cells(logo::FLAT.height() + 2);
    if area.height < tall + 2 || usize::from(area.width) < logo::FLAT.width() + 2 {
        frame.render_widget(Line::styled(format!(" {note}"), dim()), area);
        return;
    }
    let top = area.y + (area.height - tall) / 2;
    let left = area.x + (area.width - cells(logo::FLAT.width())) / 2;
    frame.render_widget(
        Paragraph::new(ticket(logo::FLAT)),
        Rect::new(
            left,
            top,
            cells(logo::FLAT.width()),
            cells(logo::FLAT.height()),
        ),
    );
    frame.render_widget(
        Line::styled(note.to_owned(), dim()).centered(),
        Rect::new(area.x, top + cells(logo::FLAT.height()) + 1, area.width, 1),
    );
}

/// Cells a small ticket crosses while something is under way.
const TRACK: usize = 8;

const FOLDER: Color = Color::Blue;
const MARK: Color = Color::Green;
const PROBLEM: Color = Color::Red;

const fn dim() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

const fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

const fn cursor() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

/// Cells, from a count that fits in any terminal.
fn cells(count: usize) -> u16 {
    u16::try_from(count).unwrap_or(u16::MAX)
}

fn width_of(text: &str) -> usize {
    text.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// `text` cut to `width` cells, with `…` where it was cut.
fn fit(text: &str, width: usize) -> String {
    if width_of(text) <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// `text` cut from the left, for paths, whose end is what matters.
fn fit_left(text: &str, width: usize) -> String {
    if width_of(text) <= width {
        return text.to_owned();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1;
    for c in text.chars().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        kept.push(c);
        used += w;
    }
    std::iter::once('…').chain(kept.into_iter().rev()).collect()
}

/// How long ago, coarsely: `now`, `5m`, `3h`, `2d`, or a date.
fn ago(when: SystemTime) -> String {
    let Ok(age) = SystemTime::now().duration_since(when) else {
        return "now".to_owned();
    };
    let seconds = age.as_secs();
    match seconds {
        0..60 => "now".to_owned(),
        60..3_600 => format!("{}m", seconds / 60),
        3_600..86_400 => format!("{}h", seconds / 3_600),
        86_400..2_592_000 => format!("{}d", seconds / 86_400),
        _ => {
            let date = time::OffsetDateTime::from(when).date();
            format!(
                "{}-{:02}-{:02}",
                date.year(),
                u8::from(date.month()),
                date.day()
            )
        }
    }
}

/// An expiry as this machine's clock reads it, `2026-10-26 09:56`; nothing
/// for a share that never expires.
fn when(expires_at: Option<&str>, offset: time::UtcOffset) -> String {
    use time::format_description::well_known::Rfc3339;

    let Some(at) = expires_at.and_then(|at| time::OffsetDateTime::parse(at, &Rfc3339).ok()) else {
        return String::new();
    };
    let at = at.to_offset(offset);
    format!(
        "{}-{:02}-{:02} {:02}:{:02}",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// Rows `offset` must move to so `cursor` is among `height` rows.
const fn scrolled(offset: usize, cursor: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    if cursor < offset {
        cursor
    } else if cursor >= offset + height {
        cursor + 1 - height
    } else {
        offset
    }
}

pub(crate) fn draw(app: &mut App, frame: &mut Frame<'_>) {
    let area = frame.area();
    let mut hits = Vec::new();
    if area.width < 30 || area.height < 8 {
        frame.render_widget(
            Paragraph::new("Make the terminal larger").wrap(Wrap { trim: true }),
            area,
        );
        app.hits = hits;
        return;
    }
    let header = Rect { height: 1, ..area };
    let footer = Rect {
        y: area.bottom().saturating_sub(1),
        height: 1,
        ..area
    };
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(2),
        ..area
    };
    draw_header(app, frame, header, &mut hits);
    match app.tab {
        Tab::Files if app.finder.is_some() => draw_finder(app, frame, body, &mut hits),
        Tab::Files => draw_files(app, frame, body, &mut hits),
        Tab::Shares => draw_shares(app, frame, body, &mut hits),
    }
    draw_footer(app, frame, footer, &mut hits);
    if app.modal.is_some() {
        draw_modal(app, frame, area, &mut hits);
    }
    if let Some(selection) = app.selection {
        selection.highlight(frame.buffer_mut());
    }
    app.hits = hits;
}

fn draw_header(app: &App, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    // The bars printed on the site's ticket, as its mark.
    let mark = " ╱╱╱ ";
    let mut x = area.x + cells(width_of(mark));
    let mut spans = vec![Span::styled(mark, bold().fg(accent()))];
    for (tab, label) in [(Tab::Files, " Files "), (Tab::Shares, " Shares ")] {
        let style = if app.tab == tab {
            bold().fg(accent()).add_modifier(Modifier::UNDERLINED)
        } else {
            dim()
        };
        let width = cells(width_of(label));
        hits.push((Rect::new(x, area.y, width, 1), Hit::Tab(tab)));
        x = x.saturating_add(width);
        spans.push(Span::styled(label, style));
    }
    frame.render_widget(Line::from(spans), area);

    let instance = match &app.client {
        Some(client) if app.unreachable.is_some() => {
            let host = client.server().host_str().unwrap_or("instance");
            let line = Line::styled(
                format!("{host} · can't be reached, F5 tries again "),
                Style::new().fg(PROBLEM),
            );
            frame.render_widget(line.right_aligned(), area);
            return;
        }
        Some(client) => {
            let host = client.server().host_str().unwrap_or("instance").to_owned();
            if client.signed_in() {
                format!("{host} · signed in ")
            } else {
                format!("{host} · not signed in ")
            }
        }
        None => "no instance yet ".to_owned(),
    };
    frame.render_widget(Line::styled(instance, dim()).right_aligned(), area);
}

/// The folder as a row of names, each a way back to it.
fn draw_crumbs(app: &App, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let dir = app.browser.dir();
    let mut crumbs: Vec<(String, std::path::PathBuf)> = Vec::new();
    let home = std::env::home_dir().filter(|home| dir.starts_with(home));
    let mut walked = std::path::PathBuf::new();
    for part in dir.components() {
        walked.push(part);
        if home
            .as_ref()
            .is_some_and(|home| home.starts_with(&walked) && *home != walked)
        {
            continue;
        }
        let label = if home.as_ref() == Some(&walked) {
            "~".to_owned()
        } else {
            match part {
                std::path::Component::RootDir => "/".to_owned(),
                other => other.as_os_str().to_string_lossy().into_owned(),
            }
        };
        crumbs.push((label, walked.clone()));
    }
    // Leave out the first ones until the rest fit.
    let separator = " / ";
    let total = |from: usize| -> usize {
        crumbs
            .iter()
            .skip(from)
            .map(|(label, _)| width_of(label) + separator.len())
            .sum::<usize>()
            + 2
    };
    let available = usize::from(area.width);
    let skip = (0..crumbs.len())
        .find(|from| total(*from) <= available)
        .unwrap_or_else(|| crumbs.len().saturating_sub(1));
    let mut spans = vec![Span::raw(" ")];
    let mut x = area.x + 1;
    if skip > 0 {
        spans.push(Span::styled("…/", dim()));
        x += 2;
    }
    let last = crumbs.len().saturating_sub(1);
    for (index, (label, path)) in crumbs.into_iter().enumerate().skip(skip) {
        let width = cells(width_of(&label));
        hits.push((Rect::new(x, area.y, width, 1), Hit::Crumb(path)));
        x = x.saturating_add(width);
        let style = if index == last { bold() } else { Style::new() };
        let joins = label != "/" && index != last;
        spans.push(Span::styled(label, style));
        if joins {
            spans.push(Span::styled(" / ", dim()));
            x = x.saturating_add(3);
        }
    }
    frame.render_widget(Line::from(spans), area);
    // How the folder is ordered, as a switch: clicking steps to the next way.
    let order = match app.browser.sort() {
        crate::config::Sort::Name => "by name",
        crate::config::Sort::Modified => "newest first",
        crate::config::Sort::Size => "largest first",
    };
    let label = format!(" {order} ");
    let width = cells(width_of(&label));
    hits.push((
        Rect::new(area.right().saturating_sub(width), area.y, width, 1),
        Hit::Button(Button::Footer(Action::Sort)),
    ));
    frame.render_widget(Line::styled(label, dim()).right_aligned(), area);
}

fn draw_files(app: &mut App, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    draw_crumbs(app, frame, Rect { height: 1, ..area }, hits);
    let filter_shown = app.filtering || !app.browser.filter().is_empty();
    let list = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1 + u16::from(filter_shown)),
        ..area
    };
    let height = usize::from(list.height);
    app.page = height.saturating_sub(1).max(1);

    if let Some(error) = &app.browser.error {
        frame.render_widget(
            Paragraph::new(format!(" Cannot read this folder: {error}"))
                .style(Style::new().fg(PROBLEM))
                .wrap(Wrap { trim: true }),
            list,
        );
    } else if app.browser.rows().iter().all(|row| *row == Row::Parent) {
        let below = Rect {
            y: list.y + cells(app.browser.rows().len()),
            height: list.height.saturating_sub(cells(app.browser.rows().len())),
            ..list
        };
        if app.browser.filter().is_empty() {
            draw_ticket_with(frame, below, "Empty folder");
        } else {
            let note = format!(" Nothing here matches “{}”", app.browser.filter());
            frame.render_widget(Line::styled(note, dim()), below);
        }
    }

    app.offsets.files = scrolled(app.offsets.files, app.browser.cursor(), height);
    draw_file_rows(app, frame, list, hits);

    if filter_shown {
        let filter_area = Rect {
            y: area.bottom().saturating_sub(1),
            height: 1,
            ..area
        };
        let count = app
            .browser
            .rows()
            .iter()
            .filter(|row| **row != Row::Parent)
            .count();
        frame.render_widget(
            Line::from(vec![
                Span::styled(" Filter ", dim()),
                Span::styled(app.browser.filter().to_owned(), bold()),
                Span::styled(format!("   {count} shown · Esc clears"), dim()),
            ]),
            filter_area,
        );
        if app.filtering && app.modal.is_none() {
            frame.set_cursor_position(Position::new(
                filter_area.x + 8 + cells(width_of(app.browser.filter())),
                filter_area.y,
            ));
        }
    }
}

/// The rows of the folder that fit in `list`, from the first on screen.
fn draw_file_rows(app: &App, frame: &mut Frame<'_>, list: Rect, hits: &mut Hits) {
    let height = usize::from(list.height);
    let width = usize::from(list.width);
    for (shown, row) in app
        .browser
        .rows()
        .iter()
        .enumerate()
        .skip(app.offsets.files)
        .take(height)
    {
        let y = list.y + cells(shown - app.offsets.files);
        let line_area = Rect::new(list.x, y, list.width, 1);
        hits.push((line_area, Hit::FileRow(shown)));
        let entry = app.browser.entry(*row);
        let marked = entry.is_some_and(|entry| app.browser.is_marked(&entry.path));
        let mark = match (row, marked) {
            (Row::Parent, _) => "    ",
            (_, true) => " ■  ",
            (_, false) => " □  ",
        };
        if *row != Row::Parent {
            hits.push((Rect::new(list.x, y, 3, 1), Hit::FileMark(shown)));
        }
        let (name, style, size, when) = match entry {
            None => (
                "..".to_owned(),
                Style::new().fg(FOLDER),
                String::new(),
                String::new(),
            ),
            Some(entry) => (
                if entry.is_dir {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                },
                if entry.is_dir {
                    bold().fg(FOLDER)
                } else {
                    Style::new()
                },
                entry.size.map(bytes).unwrap_or_default(),
                entry.modified.map(ago).unwrap_or_default(),
            ),
        };
        let name_width = width.saturating_sub(4 + 10 + 12);
        let name = fit(&name, name_width);
        let pad = name_width.saturating_sub(width_of(&name));
        let mut spans = vec![
            Span::styled(mark, if marked { bold().fg(MARK) } else { dim() }),
            Span::styled(name, style),
            Span::raw(" ".repeat(pad)),
            Span::styled(format!("{size:>10}"), dim()),
            Span::styled(format!("{when:>11} "), dim()),
        ];
        let mut line_style = Style::new();
        if shown == app.browser.cursor() && app.modal.is_none() {
            line_style = cursor();
        }
        if marked {
            spans.iter_mut().skip(1).take(1).for_each(|span| {
                span.style = span.style.fg(MARK);
            });
        }
        frame.render_widget(Line::from(spans).style(line_style), line_area);
    }
}

fn draw_finder(app: &mut App, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let Some(finder) = &app.finder else {
        return;
    };
    let top = Rect { height: 1, ..area };
    let root = tilde(finder.root());
    let prompt = format!(" Find under {} ", fit_left(&root, 30));
    let state = if finder.walking() {
        let frame_index = finder.total() / 256 % SPINNER.len();
        format!(
            "{} {} files…",
            SPINNER.get(frame_index).copied().unwrap_or(' '),
            finder.total()
        )
    } else if finder.capped() {
        format!("{} files (stopped there)", finder.total())
    } else {
        format!("{} files", finder.total())
    };
    frame.render_widget(
        Line::from(vec![
            Span::styled(prompt.clone(), dim()),
            Span::styled(finder.query().to_owned(), bold()),
        ]),
        top,
    );
    frame.render_widget(
        Line::styled(format!("{state} "), dim()).right_aligned(),
        top,
    );
    if app.modal.is_none() {
        frame.set_cursor_position(Position::new(
            top.x + cells(width_of(&prompt) + width_of(finder.query())),
            top.y,
        ));
    }

    let list = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    let height = usize::from(list.height);
    app.page = height.saturating_sub(1).max(1);
    app.offsets.found = scrolled(app.offsets.found, finder.cursor(), height);
    if finder.matches() == 0 && !finder.walking() {
        frame.render_widget(
            Line::styled(" Nothing matches", dim()),
            Rect { height: 1, ..list },
        );
    }
    for (shown, found) in finder
        .shown()
        .enumerate()
        .skip(app.offsets.found)
        .take(height)
    {
        let y = list.y + cells(shown - app.offsets.found);
        let line_area = Rect::new(list.x, y, list.width, 1);
        hits.push((line_area, Hit::FoundRow(shown)));
        hits.push((Rect::new(list.x, y, 3, 1), Hit::FoundMark(shown)));
        let marked = app.browser.is_marked(&finder.absolute(found));
        let (folder, name) = match found.path.rsplit_once('/') {
            Some((folder, name)) => (format!("{folder}/"), name.to_owned()),
            None => (String::new(), found.path.clone()),
        };
        let name = if found.is_dir {
            format!("{name}/")
        } else {
            name
        };
        let space = usize::from(list.width).saturating_sub(4 + width_of(&name));
        let style = if found.is_dir {
            bold().fg(FOLDER)
        } else if marked {
            Style::new().fg(MARK)
        } else {
            bold()
        };
        let line = Line::from(vec![
            Span::styled(
                if marked { " ■  " } else { " □  " },
                if marked { bold().fg(MARK) } else { dim() },
            ),
            Span::styled(fit_left(&folder, space), dim()),
            Span::styled(name, style),
        ]);
        let line_style = if shown == finder.cursor() && app.modal.is_none() {
            cursor()
        } else {
            Style::new()
        };
        frame.render_widget(line.style(line_style), line_area);
    }
}

fn draw_shares(app: &mut App, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let list = match &app.shares.list {
        None => {
            frame.render_widget(Line::styled(" Loading your shares…", dim()), area);
            return;
        }
        Some(Err(problem)) => {
            frame.render_widget(
                Paragraph::new(format!(" {problem}")).wrap(Wrap { trim: true }),
                area,
            );
            return;
        }
        Some(Ok(list)) => list,
    };
    if list.is_empty() {
        draw_ticket_with(frame, area, "No shares yet");
        return;
    }
    let now = time::OffsetDateTime::now_utc();
    let code_width = list
        .iter()
        .map(|share| share.code.len())
        .max()
        .unwrap_or(4)
        .max(4);
    frame.render_widget(
        Line::styled(
            format!(
                " {:<code_width$}  {:<25} {:>5}  NAME",
                "CODE", "EXPIRES", "LEFT"
            ),
            dim(),
        ),
        Rect { height: 1, ..area },
    );
    let rows = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    let height = usize::from(rows.height);
    app.page = height.saturating_sub(1).max(1);
    app.offsets.shares = scrolled(app.offsets.shares, app.shares.cursor, height);
    for (index, share) in list
        .iter()
        .enumerate()
        .skip(app.offsets.shares)
        .take(height)
    {
        let y = rows.y + cells(index - app.offsets.shares);
        let line_area = Rect::new(rows.x, y, rows.width, 1);
        hits.push((line_area, Hit::ShareRow(index)));
        let left = share
            .uses_remaining
            .map_or_else(|| "-".to_owned(), |left| left.to_string());
        let line = Line::from(vec![
            Span::styled(format!(" {:<code_width$}  ", share.code), bold()),
            Span::raw(format!(
                "{:<16}",
                when(share.expires_at.as_deref(), app.local_offset)
            )),
            Span::styled(
                format!(
                    "  {:<7} {left:>5}  ",
                    crate::shares::expires(share.expires_at.as_deref(), now)
                ),
                dim(),
            ),
            Span::raw(crate::shares::name(share)),
        ]);
        let style = if index == app.shares.cursor && app.modal.is_none() {
            cursor()
        } else {
            Style::new()
        };
        frame.render_widget(line.style(style), line_area);
    }
}

fn draw_footer(app: &App, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let actions: &[(&str, Action)] = match app.tab {
        Tab::Files if app.finder.is_some() => &[("Send", Action::Send), ("Keys", Action::Help)],
        Tab::Files => &[
            ("Send", Action::Send),
            ("New", Action::New),
            ("Find", Action::Find),
            ("Download", Action::Get),
            ("Settings", Action::Settings),
            ("Keys", Action::Help),
        ],
        Tab::Shares => &[
            ("Copy", Action::Copy),
            ("Delete", Action::Delete),
            ("Reload", Action::Refresh),
            ("Settings", Action::Settings),
            ("Keys", Action::Help),
        ],
    };
    let texts: Vec<(String, Action)> = actions
        .iter()
        .map(|(label, action)| {
            let hint = app.keymap.hint(*action).unwrap_or_default();
            (format!(" {label} {hint} "), *action)
        })
        .collect();
    // Room first for what the left says, then for as many buttons as fit
    // beside it: the first and the last, which opens the list of keys,
    // stay; those between go from the right.
    let (wanted, _) = footer_note(app, usize::MAX);
    let wanted = wanted.width();
    let mut shown = texts;
    let taken = |shown: &[(String, Action)]| -> usize {
        shown.iter().map(|(text, _)| width_of(text) + 1).sum()
    };
    while shown.len() > 2 && taken(&shown) + wanted > usize::from(area.width) {
        shown.remove(shown.len() - 2);
    }
    let mut spans = Vec::new();
    let mut width = 0;
    let mut placed = Vec::new();
    for (text, action) in shown {
        let text_width = width_of(&text);
        placed.push((width, text_width, action));
        width += text_width + 1;
        spans.push(Span::styled(text, cursor().add_modifier(Modifier::DIM)));
        spans.push(Span::raw(" "));
    }
    let start = area.right().saturating_sub(cells(width));
    for (offset, text_width, action) in placed {
        hits.push((
            Rect::new(start + cells(offset), area.y, cells(text_width), 1),
            Hit::Button(Button::Footer(action)),
        ));
    }
    frame.render_widget(Line::from(spans).right_aligned(), area);

    let left = Rect {
        width: area.width.saturating_sub(cells(width)),
        ..area
    };
    let (line, transfer) = footer_note(app, usize::from(left.width));
    if transfer {
        hits.push((left, Hit::Button(Button::Footer(Action::Transfers))));
    }
    frame.render_widget(line, left);
}

/// What the left of the footer says, in `width` cells: a message, the oldest
/// transfer under way, or what is marked. True when it is a transfer, which a
/// click opens the list of.
fn footer_note(app: &App, width: usize) -> (Line<'static>, bool) {
    if let Some((message, tone, _)) = &app.status {
        let style = match tone {
            Tone::Info => Style::new(),
            Tone::Done => Style::new().fg(MARK),
            Tone::Problem => Style::new().fg(PROBLEM),
        };
        (Line::styled(format!(" {message}"), style), false)
    } else if let Some(transfer) = app.transfers.first() {
        // Under way in the background: the oldest, and how many more.
        let progress = transfer.progress.borrow();
        let percent = progress
            .overall()
            .saturating_mul(100)
            .checked_div(progress.all.unwrap_or(progress.total))
            .unwrap_or(0);
        let more = app.transfers.len() - 1;
        let watch = app.keymap.hint(Action::Transfers).unwrap_or_default();
        let more = if more > 0 {
            format!(" · {more} more")
        } else {
            String::new()
        };
        let rest = format!(" {percent}%{more} · {watch}");
        let room = width.saturating_sub(width_of(&rest) + 2 + TRACK + 1);
        let text = format!(" {}{rest}", fit(&transfer.title, room));
        let mut spans = vec![Span::raw(" ")];
        spans.extend(motion::in_transit(
            app.started.elapsed(),
            TRACK,
            logo::truecolor(),
        ));
        spans.push(Span::styled(text, Style::new().fg(accent())));
        (Line::from(spans), true)
    } else if app.tab == Tab::Files && app.finder.is_some() {
        (
            Line::styled(" Tab marks · Enter goes there · Esc closes", dim()),
            false,
        )
    } else if app.tab == Tab::Files && app.browser.marked_count() > 0 {
        let (files, folders, size) = app.browser.marked().fold(
            (0_usize, 0_usize, 0_u64),
            |(files, folders, size), (_, kind)| match kind {
                Marked::File(bytes) => (files + 1, folders, size + bytes),
                Marked::Folder => (files, folders + 1, size),
            },
        );
        let mut parts = Vec::new();
        if files > 0 {
            parts.push(format!("{files} file{} · {}", plural(files), bytes(size)));
        }
        if folders > 0 {
            parts.push(format!("{folders} folder{}", plural(folders)));
        }
        (
            Line::styled(format!(" {} marked", parts.join(" + ")), bold().fg(MARK)),
            false,
        )
    } else if app.tab == Tab::Files && app.keymap.type_to_filter {
        (Line::styled(" Type to filter · Space marks", dim()), false)
    } else {
        (Line::default(), false)
    }
}

const fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

// --- dialogs ---

/// Clears a box `width` by `height` in the middle of `area`, draws its frame,
/// and gives back the inside.
fn dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    width: u16,
    height: u16,
    title: &str,
    hits: &mut Hits,
) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let outer = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, outer);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(accent()))
        .title(Span::styled(format!(" {title} "), bold()));
    let inner = block.inner(outer);
    frame.render_widget(block, outer);
    hits.push((outer, Hit::Dialog));
    Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    }
}

/// Buttons along the bottom of `inner`, right-aligned.
fn buttons(frame: &mut Frame<'_>, inner: Rect, list: &[(&str, Button, bool)], hits: &mut Hits) {
    let row = Rect {
        y: inner.bottom().saturating_sub(1),
        height: 1,
        ..inner
    };
    let total: usize = list.iter().map(|(label, _, _)| width_of(label) + 5).sum();
    let mut x = row.right().saturating_sub(cells(total));
    let mut spans = Vec::new();
    for (label, button, focused) in list {
        let text = format!("[ {label} ]");
        let width = cells(width_of(&text));
        hits.push((Rect::new(x, row.y, width, 1), Hit::Button(*button)));
        x = x.saturating_add(width + 1);
        let style = if *focused {
            bold().fg(accent()).add_modifier(Modifier::REVERSED)
        } else {
            bold()
        };
        spans.push(Span::styled(text, style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Line::from(spans).right_aligned(), row);
}

/// Rows `text` takes wrapped to `width`, roughly as `Paragraph` wraps it.
fn rows_for(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let rows: usize = text
        .lines()
        .map(|line| width_of(line).div_ceil(width).max(1))
        .sum();
    cells(rows.max(1))
}

type Hits = Vec<(Rect, Hit)>;

fn draw_modal(app: &App, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let Some(modal) = &app.modal else {
        return;
    };
    match modal {
        Modal::Send(form) => draw_form(form, frame, area, hits),
        Modal::Compose(compose) => draw_compose(compose, frame, area, hits),
        Modal::Share { share, qr } => draw_share(app, share, *qr, frame, area, hits),
        Modal::Transfers(row) => draw_transfers(app, *row, frame, area, hits),
        Modal::Sending(id) => {
            if let Some(transfer) = app.transfers.iter().find(|transfer| transfer.id == *id) {
                draw_sending(app, transfer, frame, area, hits);
            }
        }
        Modal::Sent {
            link,
            what,
            skipped,
            qr,
        } => {
            let copy = app.keymap.hint(Action::Copy).unwrap_or_default();
            let code = qr.then(|| app.qr_code(link, accent())).flatten();
            draw_sent(
                frame,
                area,
                hits,
                (link, what, *skipped, code.as_deref()),
                (
                    &copy,
                    app.choice,
                    app.sent_at.map(|at| at.elapsed()).unwrap_or_default(),
                ),
            );
        }
        Modal::Got { dir, files } => draw_got(frame, area, hits, dir, files),
        Modal::Note(text) => draw_note(frame, area, hits, text, app.choice),
        Modal::Failed { title, message } => {
            let width = 64.min(area.width);
            let rows = rows_for(message, width.saturating_sub(4));
            let inner = dialog(frame, area, width, rows + 4, title, hits);
            frame.render_widget(
                Paragraph::new(message.clone()).wrap(Wrap { trim: false }),
                inner,
            );
            buttons(frame, inner, &[("Close", Button::Close, true)], hits);
        }
        Modal::Prompt(prompt) => draw_prompt(app, prompt, frame, area, hits),
        Modal::Confirm(confirm) => draw_confirm(confirm, app.choice, frame, area, hits),
        Modal::Menu(menu) => draw_menu(menu, frame, area, hits),
        Modal::Help { scroll } => draw_help(app, *scroll, frame, area, hits),
        Modal::Settings(row) => draw_settings(app, *row, frame, area, hits),
        Modal::Welcome => draw_welcome(app, frame, area, hits),
        Modal::NeedSignIn { title, message } => {
            let text = format!(
                "{message}\n\nSigning in opens a link to approve this machine, on any device."
            );
            let width = 64.min(area.width);
            let rows = rows_for(&text, width.saturating_sub(4));
            let inner = dialog(frame, area, width, rows + 4, title, hits);
            frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
            buttons(
                frame,
                inner,
                &[
                    ("Sign in", Button::SignIn, app.choice == 0),
                    ("Close", Button::Close, app.choice == 1),
                ],
                hits,
            );
        }
        Modal::SignIn(sign_in) => {
            let code = (!sign_in.link.is_empty())
                .then(|| app.qr_code(&sign_in.link, accent()))
                .flatten();
            draw_sign_in(sign_in, code.as_deref(), app.choice, frame, area, hits);
        }
    }
}

/// The text of a line, without its styles.
fn plain(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// A dialog of `lines`, with a QR code of `qr` when there is one: beside the
/// text when the screen is wide enough, under it when it is tall enough, and
/// otherwise a line saying so, since a code cut short does not scan. Gives
/// the inside of the dialog, for its buttons along the bottom.
fn with_qr(
    frame: &mut Frame<'_>,
    area: Rect,
    hits: &mut Hits,
    (title, width): (&str, u16),
    mut lines: Vec<Line<'static>>,
    qr: Option<(&super::qr::Drawn, f32)>,
) -> Rect {
    // Around the text: a border and a column of space on each side, and below
    // it, a blank row, the buttons and the border.
    const SIDES: u16 = 4;
    const ENDS: u16 = 4;
    let room = (area.width.saturating_sub(2), area.height.saturating_sub(2));
    let rows = |lines: &[Line<'_>], width: u16| -> u16 {
        lines.iter().map(|line| rows_for(&plain(line), width)).sum()
    };
    let natural = lines
        .iter()
        .map(|line| cells(width_of(&plain(line))))
        .max()
        .unwrap_or(0);
    let narrow = width.min(room.0).saturating_sub(SIDES);
    let Some((drawn, printed)) = qr else {
        let height = rows(&lines, narrow) + ENDS;
        let inner = dialog(frame, area, width, height, title, hits);
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        return inner;
    };
    let (qr_width, qr_height) = (drawn.width, drawn.height);
    let beside = room
        .0
        .saturating_sub(SIDES + 2 + qr_width)
        .min(natural.max(narrow.min(48)));
    let beside_height = rows(&lines, beside).max(qr_height) + ENDS;
    if beside >= 30 && beside_height <= room.1 {
        let inner = dialog(
            frame,
            area,
            beside + 2 + qr_width + SIDES,
            beside_height,
            title,
            hits,
        );
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            Rect {
                width: beside,
                ..inner
            },
        );
        let at = Rect::new(
            inner.right().saturating_sub(qr_width),
            inner.y,
            qr_width,
            qr_height,
        );
        drawn.render(frame, at, printed);
        return inner;
    }
    let text_rows = rows(&lines, narrow);
    let below_height = text_rows + 1 + qr_height + ENDS;
    if below_height <= room.1 && qr_width <= room.0.saturating_sub(SIDES) {
        let inner = dialog(
            frame,
            area,
            width.max(qr_width + SIDES),
            below_height,
            title,
            hits,
        );
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            Rect {
                height: text_rows,
                ..inner
            },
        );
        let at = Rect::new(
            inner.x + inner.width.saturating_sub(qr_width) / 2,
            inner.y + text_rows + 1,
            qr_width,
            qr_height,
        );
        drawn.render(frame, at, printed);
        return inner;
    }
    lines.push(Line::default());
    lines.push(Line::styled(
        "A larger window would show the QR code whole.",
        dim(),
    ));
    let height = rows(&lines, narrow) + ENDS;
    let inner = dialog(frame, area, width, height, title, hits);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    inner
}

fn draw_sent(
    frame: &mut Frame<'_>,
    area: Rect,
    hits: &mut Hits,
    (link, what, skipped, qr): (&str, &str, usize, Option<&super::qr::Drawn>),
    (copy, choice, torn): (&str, usize, std::time::Duration),
) {
    let typed = motion::typed(link, torn);
    let mut lines = vec![
        Line::styled(what.to_owned(), dim()),
        Line::default(),
        // The rest of the link held open as space, so that typing it out
        // does not widen the dialog as it goes.
        Line::from(vec![
            Span::styled(
                typed.clone(),
                bold().fg(accent()).add_modifier(Modifier::UNDERLINED),
            ),
            Span::raw(" ".repeat(width_of(link).saturating_sub(width_of(&typed)))),
        ]),
    ];
    if skipped > 0 {
        lines.push(Line::default());
        lines.push(Line::styled(
            format!("Left out {skipped} empty file{}.", plural(skipped)),
            dim(),
        ));
    }
    lines.push(Line::default());
    lines.extend(motion::torn(torn, logo::truecolor()));
    let printed = motion::printed(torn);
    let inner = with_qr(
        frame,
        area,
        hits,
        ("Sent", 72),
        lines,
        qr.map(|qr| (qr, printed)),
    );
    buttons(
        frame,
        inner,
        &[
            (&format!("Copy link {copy}"), Button::CopyLink, choice == 0),
            (
                if qr.is_some() { "Hide QR" } else { "QR code" },
                Button::ToggleQr,
                choice == 1,
            ),
            ("Close", Button::Close, choice == 2),
        ],
        hits,
    );
}

fn draw_got(
    frame: &mut Frame<'_>,
    area: Rect,
    hits: &mut Hits,
    dir: &std::path::Path,
    files: &[std::path::PathBuf],
) {
    let shown = files.len().min(12);
    let height = 6 + cells(shown);
    let inner = dialog(frame, area, 64, height, "Downloaded", hits);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Into ", dim()),
            Span::styled(fit_left(&tilde(dir), 50), bold()),
        ]),
        Line::default(),
    ];
    for file in files.iter().take(shown) {
        let name = file.strip_prefix(dir).unwrap_or(file).display().to_string();
        lines.push(Line::raw(name));
    }
    if files.len() > shown {
        lines.push(Line::styled(
            format!("and {} more", files.len() - shown),
            dim(),
        ));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    buttons(frame, inner, &[("Close", Button::Close, true)], hits);
}

fn draw_note(frame: &mut Frame<'_>, area: Rect, hits: &mut Hits, text: &str, choice: usize) {
    let width = 72.min(area.width);
    let rows = rows_for(text, width.saturating_sub(4)).min(area.height.saturating_sub(8));
    let inner = dialog(frame, area, width, rows + 4, "Note", hits);
    frame.render_widget(
        Paragraph::new(text.to_owned()).wrap(Wrap { trim: false }),
        Rect {
            height: inner.height.saturating_sub(2),
            ..inner
        },
    );
    buttons(
        frame,
        inner,
        &[
            ("Copy", Button::CopyLink, choice == 0),
            ("Close", Button::Close, choice == 1),
        ],
        hits,
    );
}

fn draw_prompt(app: &App, prompt: &Prompt, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let (title, question) = match &prompt.ask {
        Ask::GoTo => (
            "Go to",
            "A folder or a file, absolute, relative or from ~".to_owned(),
        ),
        Ask::Link => (
            "Download",
            format!(
                "A link or a code, to download into {}",
                fit_left(&tilde(app.browser.dir()), 30)
            ),
        ),
        Ask::Password { .. } => ("Password", "This share has a password".to_owned()),
        Ask::Target => (
            "Link",
            "A web address; the share sends its visitors there".to_owned(),
        ),
        Ask::Instance => (
            "Sign in",
            "The instance's address, like share.example.com".to_owned(),
        ),
    };
    let height = 7 + u16::from(prompt.error.is_some());
    let inner = dialog(frame, area, 68, height, title, hits);
    frame.render_widget(Line::styled(question, dim()), Rect { height: 1, ..inner });
    let field = Rect {
        y: inner.y + 2,
        height: 1,
        ..inner
    };
    let shown = prompt.input.shown();
    let before: String = shown.chars().take(prompt.input.caret()).collect();
    let visible = fit_left(&before, usize::from(field.width.saturating_sub(2)));
    let after: String = shown.chars().skip(prompt.input.caret()).collect();
    frame.render_widget(
        Line::from(vec![
            Span::styled("› ", Style::new().fg(accent())),
            Span::raw(visible.clone()),
            Span::raw(after),
        ]),
        field,
    );
    frame.set_cursor_position(Position::new(
        field.x + 2 + cells(width_of(&visible)),
        field.y,
    ));
    if let Some(error) = &prompt.error {
        frame.render_widget(
            Line::styled(error.clone(), Style::new().fg(PROBLEM)),
            Rect {
                y: field.y + 1,
                ..field
            },
        );
    }
    buttons(
        frame,
        inner,
        &[("OK", Button::Ok, true), ("Cancel", Button::Close, false)],
        hits,
    );
}

fn draw_confirm(
    confirm: &Confirm,
    choice: usize,
    frame: &mut Frame<'_>,
    area: Rect,
    hits: &mut Hits,
) {
    let (title, text, yes, no) = match confirm {
        Confirm::Delete { code, name } => (
            "Delete",
            format!("Delete {code} ({name})? The link stops working for everyone."),
            "Delete",
            "Keep",
        ),
        Confirm::Stop { which, then_quit } => (
            "Stop",
            match (which, then_quit) {
                (None, true) => {
                    "Stop everything under way and quit? Sending the same files \
                                 again picks up where it stopped."
                }
                (None, false) => {
                    "Stop everything under way? Sending the same files again \
                                  picks up where it stopped."
                }
                (Some(_), _) => {
                    "Stop this one? Sending the same files again picks up where \
                                 it stopped."
                }
            }
            .to_owned(),
            "Stop",
            "Go on",
        ),
    };
    let rows = rows_for(&text, 52);
    let inner = dialog(frame, area, 56, rows + 4, title, hits);
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), inner);
    buttons(
        frame,
        inner,
        &[
            (&format!("{yes} y"), Button::Yes, choice == 0),
            (&format!("{no} n"), Button::No, choice == 1),
        ],
        hits,
    );
}

fn draw_menu(menu: &Menu, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let width = cells(
        menu.items
            .iter()
            .map(|(label, _)| width_of(label))
            .max()
            .unwrap_or(0)
            + 4,
    );
    let height = cells(menu.items.len() + 2);
    let x = menu.at.x.min(area.right().saturating_sub(width));
    let y = menu.at.y.min(area.bottom().saturating_sub(height));
    let outer = Rect::new(x, y, width.min(area.width), height.min(area.height));
    frame.render_widget(Clear, outer);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(accent()));
    let inner = block.inner(outer);
    frame.render_widget(block, outer);
    hits.push((outer, Hit::Dialog));
    for (index, (label, _)) in menu.items.iter().enumerate() {
        let row = Rect::new(inner.x, inner.y + cells(index), inner.width, 1);
        if row.y >= inner.bottom() {
            break;
        }
        hits.push((row, Hit::Button(Button::MenuItem(index))));
        let style = if index == menu.cursor {
            cursor()
        } else {
            Style::new()
        };
        frame.render_widget(Line::styled(format!(" {label} "), style), row);
    }
}

/// The `[ui]` table, a row to a setting, changed with the arrows.
fn draw_settings(app: &App, row: usize, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let ui = &app.ui;
    let rows = Setting::ALL.map(|setting| match setting {
        Setting::Keys => (
            "Keys",
            match ui.keys {
                crate::config::Preset::Standard => "standard",
                crate::config::Preset::Vim => "vim",
            }
            .to_owned(),
            "hjkl and letters, or arrows and Ctrl",
        ),
        Setting::Mouse => (
            "Mouse",
            if ui.mouse { "on" } else { "off" }.to_owned(),
            "off: the terminal selects text itself",
        ),
        Setting::Dotfiles => (
            "Dotfiles",
            if ui.show_hidden { "shown" } else { "hidden" }.to_owned(),
            "files whose names start with a dot",
        ),
        Setting::Order => (
            "Order",
            ui.sort.as_str().to_owned(),
            "folders first, then by this",
        ),
        Setting::Colour => (
            "Colour",
            ui.accent.clone().unwrap_or_else(|| "the site's".to_owned()),
            "picking one overrides the site's",
        ),
        Setting::Account => (
            "Account",
            match &app.client {
                Some(client) if client.signed_in() => "signed in".to_owned(),
                _ => "sign in".to_owned(),
            },
            "approve this machine in a browser",
        ),
    });
    let config = crate::config::default_path()
        .map(|path| tilde(&path))
        .unwrap_or_default();
    let inner = dialog(frame, area, 70, cells(rows.len() + 6), "Settings", hits);
    for (index, (label, value, note)) in rows.into_iter().enumerate() {
        let line_area = Rect::new(inner.x, inner.y + cells(index), inner.width, 1);
        hits.push((line_area, Hit::Button(Button::Setting(index))));
        let focused = index == row;
        let pointer = if focused { "› " } else { "  " };
        let label_style = if focused { bold().fg(accent()) } else { dim() };
        frame.render_widget(
            Line::from(vec![
                Span::styled(format!("{pointer}{label:<10}"), label_style),
                Span::styled("‹ ", Style::new().fg(accent())),
                Span::styled(format!("{value:<10}"), bold()),
                Span::styled(" ›", Style::new().fg(accent())),
                Span::styled(format!("  {note}"), dim()),
            ]),
            line_area,
        );
    }
    frame.render_widget(
        Line::styled(format!("↑↓ picks · ←→ changes · saved to {config}"), dim()),
        Rect::new(inner.x, inner.y + cells(SETTINGS + 1), inner.width, 1),
    );
    buttons(frame, inner, &[("Close", Button::Close, true)], hits);
}

/// What is known of `share`, a line a fact, and its link.
fn share_lines(app: &App, share: &crate::client::OwnShare) -> (Vec<Line<'static>>, String) {
    let row = |label: &'static str, value: String| {
        Line::from(vec![
            Span::styled(format!("{label:<10}"), dim()),
            Span::raw(value),
        ])
    };
    let now = time::OffsetDateTime::now_utc();
    let mut lines = vec![row("Name", crate::shares::name(share))];
    let kind = match (share.kind.as_deref(), share.language.as_deref()) {
        (Some("link"), _) => "a link, sending visitors on".to_owned(),
        (Some("text"), Some(language)) => format!("text, {language}"),
        (Some("text"), None) => "text".to_owned(),
        _ => match share.files.len() {
            1 => "a file".to_owned(),
            count => format!("{count} files"),
        },
    };
    let size = share.size_bytes.map(bytes).unwrap_or_default();
    lines.push(row(
        "Kind",
        if size.is_empty() {
            kind
        } else {
            format!("{kind} · {size}")
        },
    ));
    let names: Vec<&str> = share
        .files
        .iter()
        .filter_map(|file| file.filename.as_deref())
        .collect();
    for (index, name) in names.iter().take(6).enumerate() {
        lines.push(row(
            if index == 0 { "Files" } else { "" },
            (*name).to_owned(),
        ));
    }
    if names.len() > 6 {
        lines.push(row("", format!("and {} more", names.len() - 6)));
    }
    let expires = match share.expires_at.as_deref() {
        Some(at) => format!(
            "{}  ({})",
            when(Some(at), app.local_offset),
            crate::shares::expires(Some(at), now)
        ),
        None => "never".to_owned(),
    };
    if let Some(made) = share.created_at.as_deref() {
        lines.push(row("Made", when(Some(made), app.local_offset)));
    }
    lines.push(row("Expires", expires));
    let left = share
        .uses_remaining
        .map_or_else(|| "no limit".to_owned(), |left| format!("{left} left"));
    let used = share.use_count.unwrap_or(0);
    lines.push(row("Downloads", format!("{used} so far · {left}")));
    if share.has_password {
        lines.push(row("Password", "asked before download".to_owned()));
    }
    let sealed = share.encryption.is_some();
    lines.push(row(
        "Sealed",
        match share.encryption.as_deref() {
            Some("password") => "yes, with a password".to_owned(),
            Some(_) => "yes, with the key in its link".to_owned(),
            None => "no".to_owned(),
        },
    ));
    let link = app
        .client
        .as_ref()
        .map(|client| crate::link::to_share(client.server(), &share.code, None))
        .unwrap_or_default();
    lines.push(Line::default());
    lines.push(Line::styled(
        link.clone(),
        bold().fg(accent()).add_modifier(Modifier::UNDERLINED),
    ));
    if sealed && share.encryption.as_deref() != Some("password") {
        lines.push(Line::styled(
            "Without its key: it opens only with the link it was sent with.",
            dim(),
        ));
    }
    (lines, link)
}

/// One of the account's shares, in full, with its link.
fn draw_share(
    app: &App,
    share: &crate::client::OwnShare,
    qr: bool,
    frame: &mut Frame<'_>,
    area: Rect,
    hits: &mut Hits,
) {
    let (lines, link) = share_lines(app, share);
    let title = format!("Share {}", share.code);
    let code = qr.then(|| app.qr_code(&link, accent())).flatten();
    let inner = with_qr(
        frame,
        area,
        hits,
        (&title, 72),
        lines,
        code.as_deref().map(|code| (code, 1.0)),
    );
    buttons(
        frame,
        inner,
        &[
            ("Copy link", Button::CopyLink, app.choice == 0),
            (
                if qr { "Hide QR" } else { "QR code" },
                Button::ToggleQr,
                app.choice == 1,
            ),
            ("Delete", Button::DeleteShare, app.choice == 2),
            ("Close", Button::Close, app.choice == 3),
        ],
        hits,
    );
}

/// A note being written: its title, its language, and what it says.
fn draw_compose(compose: &Compose, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let height = area.height.saturating_sub(4).clamp(10, 30);
    let inner = dialog(frame, area, 80, height, "Text", hits);
    let label = |field: ComposeField, text: &'static str| {
        if compose.focus == field {
            Span::styled(format!("› {text:<10}"), bold().fg(accent()))
        } else {
            Span::styled(format!("  {text:<10}"), dim())
        }
    };
    let title_row = Rect { height: 1, ..inner };
    frame.render_widget(
        Line::from(vec![
            label(ComposeField::Title, "Title"),
            Span::raw(compose.title.text().to_owned()),
            Span::styled(
                if compose.title.text().is_empty() {
                    "optional, like notes.md"
                } else {
                    ""
                },
                dim(),
            ),
        ]),
        title_row,
    );
    if compose.focus == ComposeField::Title {
        let before: String = compose
            .title
            .text()
            .chars()
            .take(compose.title.caret())
            .collect();
        frame.set_cursor_position(Position::new(
            title_row.x + LABEL + cells(width_of(&before)),
            title_row.y,
        ));
    }
    let picked = LANGUAGES.get(compose.language).copied().unwrap_or_default();
    let language = match (compose.language, compose.language()) {
        (0, Some(found)) => format!("from the title: {found}"),
        (0, None) => "none".to_owned(),
        _ => picked.to_owned(),
    };
    frame.render_widget(
        Line::from(vec![
            label(ComposeField::Language, "Language"),
            Span::styled("‹ ", Style::new().fg(accent())),
            Span::raw(language),
            Span::styled(" ›", Style::new().fg(accent())),
        ]),
        Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        },
    );
    let body = Rect {
        y: inner.y + 3,
        height: inner.height.saturating_sub(5),
        ..inner
    };
    // A body ending in a line break has the caret on the empty line after it.
    let wrapped = rows_for(&compose.body, body.width) + u16::from(compose.body.ends_with('\n'));
    let scroll = wrapped.saturating_sub(body.height);
    let text = if compose.body.is_empty() && compose.focus != ComposeField::Body {
        Paragraph::new(Line::styled("Write or paste here", dim()))
    } else {
        Paragraph::new(compose.body.clone())
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
    };
    frame.render_widget(text, body);
    if compose.focus == ComposeField::Body {
        let last = compose.body.rsplit('\n').next().unwrap_or_default();
        let width = usize::from(body.width.max(1));
        let column = width_of(last) % width;
        let row = wrapped.saturating_sub(scroll).saturating_sub(1);
        frame.set_cursor_position(Position::new(
            body.x + cells(column),
            body.y + row.min(body.height.saturating_sub(1)),
        ));
    }
    frame.render_widget(
        Line::styled(
            "Tab moves · Ctrl-S goes on to sending · Esc throws it away",
            dim(),
        ),
        Rect {
            y: inner.bottom().saturating_sub(1),
            height: 1,
            ..inner
        },
    );
}

/// What opens first when there is no instance to send to: the ticket, and
/// how to get one.
/// A sign-in waiting on a browser: the code to check, where to go, and the
/// same as a QR code for a phone, when there is room.
fn draw_sign_in(
    sign_in: &SignIn,
    qr: Option<&super::qr::Drawn>,
    choice: usize,
    frame: &mut Frame<'_>,
    area: Rect,
    hits: &mut Hits,
) {
    let title = format!("Sign in to {}", sign_in.host);
    if sign_in.code.is_empty() {
        let inner = dialog(frame, area, 60, 5, &title, hits);
        frame.render_widget(
            Line::styled(format!("Asking {}…", sign_in.host), dim()),
            inner,
        );
        buttons(frame, inner, &[("Cancel", Button::Close, true)], hits);
        return;
    }
    let lines = vec![
        Line::styled(
            "Open the link wherever you are signed in, a phone will do, and check that it \
             shows the same code:",
            dim(),
        ),
        Line::default(),
        Line::from(vec![
            Span::styled("Code  ", dim()),
            Span::styled(sign_in.code.clone(), bold().fg(accent())),
        ]),
        Line::from(vec![
            Span::styled("Link  ", dim()),
            Span::styled(
                sign_in.link.clone(),
                Style::new().fg(accent()).add_modifier(Modifier::UNDERLINED),
            ),
        ]),
        Line::default(),
        Line::from({
            let mut spans = motion::in_transit(sign_in.started.elapsed(), TRACK, logo::truecolor());
            spans.push(Span::styled(" Waiting for approval…", dim()));
            spans
        }),
    ];
    let inner = with_qr(
        frame,
        area,
        hits,
        (&title, 72),
        lines,
        qr.map(|qr| (qr, 1.0)),
    );
    buttons(
        frame,
        inner,
        &[
            ("Copy link", Button::CopyLink, choice == 0),
            ("Cancel", Button::Close, choice == 1),
        ],
        hits,
    );
}

fn draw_welcome(app: &App, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let choice = app.choice;
    let text = vec![
        Line::styled("tessera", bold().fg(accent())).centered(),
        Line::styled("Send files from here; the other side gets a link.", dim()).centered(),
        Line::default(),
        Line::styled(
            "Sign in to send. Browsing and finding files work without.",
            dim(),
        )
        .centered(),
        Line::default(),
        Line::from(vec![
            Span::styled("From scripts: ", dim()),
            Span::styled("tess send", bold()),
            Span::styled(", ", dim()),
            Span::styled("tess get", bold()),
            Span::styled(", ", dim()),
            Span::styled("tess --help", bold()),
        ])
        .centered(),
    ];
    // The tilted ticket, as on the site, when the screen holds it; flat when
    // only that fits; none on a small screen.
    let picture = [logo::TILTED, logo::FLAT].into_iter().find(|picture| {
        area.height >= cells(picture.height() + text.len() + 7)
            && area.width >= cells(picture.width() + 8)
    });
    let width = cells(picture.map_or(0, logo::Picture::width) + 6).max(64);
    let height = cells(text.len() + 4) + picture.map_or(0, |picture| cells(picture.height() + 1));
    let inner = dialog(frame, area, width, height, "Welcome", hits);
    let mut y = inner.y;
    if let Some(picture) = picture {
        let left = inner.x + inner.width.saturating_sub(cells(picture.width())) / 2;
        frame.render_widget(
            Paragraph::new(motion::shining(
                picture,
                app.started.elapsed(),
                logo::truecolor(),
            )),
            Rect::new(
                left,
                y,
                cells(picture.width()).min(inner.width),
                cells(picture.height()),
            ),
        );
        y += cells(picture.height() + 1);
    }
    frame.render_widget(
        Paragraph::new(text),
        Rect {
            y,
            height: inner.bottom().saturating_sub(y + 1),
            ..inner
        },
    );
    buttons(
        frame,
        inner,
        &[
            ("Sign in", Button::SignIn, choice == 0),
            ("Browse", Button::Close, choice == 1),
        ],
        hits,
    );
}

/// Cells the send form's labels take, the pointer and a space after included.
const LABEL: u16 = 12;

fn draw_form(form: &Form, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let height = 12 + u16::from(form.error.is_some());
    let inner = dialog(frame, area, 64, height, "Send", hits);
    frame.render_widget(
        Line::styled(fit(&form.summary, usize::from(inner.width)), bold()),
        Rect { height: 1, ..inner },
    );
    // The pointer shows where the keys go; the label keeps to LABEL cells either
    // way, which the field positions below count on.
    let label = |field: Field, text: &'static str| {
        if form.focus == field {
            Span::styled(format!("› {text:<10}"), bold().fg(accent()))
        } else {
            Span::styled(format!("  {text:<10}"), dim())
        }
    };
    let row = |offset: u16| Rect {
        y: inner.y + offset,
        height: 1,
        ..inner
    };

    let encrypt = row(2);
    hits.push((encrypt, Hit::Button(Button::Field(Field::Encrypt))));
    frame.render_widget(
        Line::from(vec![
            label(Field::Encrypt, "Encrypt"),
            Span::styled(
                if form.encrypt { "■ yes" } else { "□ no" },
                if form.encrypt {
                    bold().fg(MARK)
                } else {
                    Style::new()
                },
            ),
            Span::styled(
                if form.encryption {
                    "  sealed here; the instance never sees it"
                } else {
                    "  this instance does not offer it"
                },
                dim(),
            ),
        ]),
        encrypt,
    );

    draw_password(
        form,
        frame,
        row(3),
        label(Field::Password, "Password"),
        hits,
    );

    draw_expires(form, frame, row(4), label(Field::Expires, "Expires"), hits);

    let uses = row(5);
    hits.push((uses, Hit::Button(Button::Field(Field::Uses))));
    frame.render_widget(
        Line::from(vec![
            label(Field::Uses, "Downloads"),
            Span::raw(form.uses.text().to_owned()),
            Span::styled(
                if form.uses.text().is_empty() {
                    "no limit"
                } else {
                    "  then it stops working"
                },
                dim(),
            ),
        ]),
        uses,
    );
    if form.focus == Field::Uses {
        frame.set_cursor_position(Position::new(
            uses.x + LABEL + cells(form.uses.caret()),
            uses.y,
        ));
    }

    if let Some(error) = &form.error {
        frame.render_widget(
            Line::styled(error.clone(), Style::new().fg(PROBLEM)),
            row(7),
        );
    }
    frame.render_widget(
        Line::styled("↑↓ moves · Space or ←→ changes · Enter sends", dim()),
        row(6 + 1 + u16::from(form.error.is_some())),
    );
    buttons(
        frame,
        inner,
        &[
            (
                "Send",
                Button::Field(Field::Send),
                form.focus == Field::Send,
            ),
            (
                "Cancel",
                Button::Field(Field::Cancel),
                form.focus == Field::Cancel,
            ),
        ],
        hits,
    );
}

/// The password row of the send form, saying what the password will do.
fn draw_password(
    form: &Form,
    frame: &mut Frame<'_>,
    password: Rect,
    label: Span<'static>,
    hits: &mut Hits,
) {
    hits.push((password, Hit::Button(Button::Field(Field::Password))));
    let shown = form.password.shown();
    let typed = form.password.text().chars().count();
    let note = if typed == 0 {
        "none".to_owned()
    } else if form.encrypt && typed < tessera_crypto::MIN_PASSWORD_CHARS {
        format!(
            "  {typed}/{}: the key comes from it",
            tessera_crypto::MIN_PASSWORD_CHARS
        )
    } else if form.encrypt {
        "  the key comes from it".to_owned()
    } else {
        "  asked before download".to_owned()
    };
    frame.render_widget(
        Line::from(vec![
            label,
            Span::raw(shown.clone()),
            Span::styled(note, dim()),
        ]),
        password,
    );
    if form.focus == Field::Password {
        let before: String = shown.chars().take(form.password.caret()).collect();
        frame.set_cursor_position(Position::new(
            password.x + LABEL + cells(width_of(&before)),
            password.y,
        ));
    }
}

/// The expiry row of the send form, with arrows to click through the choices.
fn draw_expires(
    form: &Form,
    frame: &mut Frame<'_>,
    expires: Rect,
    label: Span<'static>,
    hits: &mut Hits,
) {
    let choice = form
        .expiries
        .get(form.expiry)
        .map_or("", |(_, label)| label.as_str());
    hits.push((
        Rect {
            width: LABEL + 2,
            ..expires
        },
        Hit::Button(Button::Field(Field::Expires)),
    ));
    hits.push((
        Rect::new(expires.x + LABEL, expires.y, 2, 1),
        Hit::Button(Button::ExpiresPrevious),
    ));
    let next_at = expires.x + LABEL + 2 + cells(width_of(choice)) + 1;
    hits.push((
        Rect::new(next_at, expires.y, 2, 1),
        Hit::Button(Button::ExpiresNext),
    ));
    frame.render_widget(
        Line::from(vec![
            label,
            Span::styled("‹ ", Style::new().fg(accent())),
            Span::raw(choice.to_owned()),
            Span::styled(" ›", Style::new().fg(accent())),
        ]),
        expires,
    );
}

/// How far `transfer` has got: its file, a bar, and bytes and speed.
fn transfer_lines(transfer: &Transfer, width: usize) -> [Line<'static>; 2] {
    let progress = transfer.progress.borrow();
    let (filled, empty) = crate::style::bar(progress.done, progress.total, width.saturating_sub(7));
    let percent = progress
        .done
        .saturating_mul(100)
        .checked_div(progress.total)
        .unwrap_or(0);
    let overall = progress.overall();
    let elapsed = transfer.started.elapsed().as_millis().max(1);
    let rate = if elapsed > 500 {
        // Whole bytes are all a rate needs.
        let per_second = u64::try_from(u128::from(overall) * 1000 / elapsed).unwrap_or(u64::MAX);
        format!(" · {}/s", bytes(per_second))
    } else {
        String::new()
    };
    let total = match progress.all {
        Some(all) => format!("{} of {}{rate}", bytes(overall), bytes(all)),
        None => format!("{}{rate}", bytes(overall)),
    };
    let name = if progress.name.is_empty() {
        "Starting…".to_owned()
    } else {
        progress.name.clone()
    };
    [
        Line::from(vec![
            Span::styled(filled, Style::new().fg(accent())),
            Span::styled(empty, dim()),
            Span::raw(format!(" {percent:>3}%")),
        ]),
        Line::styled(
            format!("{} · {total}", fit(&name, width.saturating_sub(30))),
            dim(),
        ),
    ]
}

/// One transfer as it goes: the small ticket riding a track as far as it
/// has got, and under it how much, how fast and what is going now.
fn draw_sending(
    app: &App,
    transfer: &Transfer,
    frame: &mut Frame<'_>,
    area: Rect,
    hits: &mut Hits,
) {
    let (verb, what) = transfer
        .title
        .split_once(' ')
        .unwrap_or((&transfer.title, ""));
    let inner = dialog(frame, area, 72, 16, verb, hits);
    let width = usize::from(inner.width);
    let done = {
        let progress = transfer.progress.borrow();
        progress.all.filter(|all| *all > 0).map(|all| {
            #[expect(
                clippy::cast_precision_loss,
                clippy::as_conversions,
                reason = "a fraction for drawing, where a few bits of a byte count are no loss"
            )]
            let fraction = progress.overall() as f32 / all as f32;
            fraction
        })
    };
    let mut lines = vec![Line::styled(fit(what, width), dim()), Line::default()];
    lines.extend(motion::riding(
        done,
        transfer.started.elapsed(),
        width,
        logo::truecolor(),
    ));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "a cell count, from a fraction between 0 and 1"
    )]
    let passed = done.map_or(0, |done| {
        (width as f32 * done.clamp(0.0, 1.0)).round() as usize
    });
    lines.push(Line::from(vec![
        Span::styled("━".repeat(passed), Style::new().fg(accent())),
        Span::styled("─".repeat(width.saturating_sub(passed)), dim()),
    ]));
    let [_, detail] = transfer_lines(transfer, width);
    let percent = done.map_or_else(String::new, |done| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions,
            reason = "a percentage, from a fraction between 0 and 1"
        )]
        let percent = (done.clamp(0.0, 1.0) * 100.0) as u8;
        format!("{percent}%  ")
    });
    let mut detail_spans = vec![Span::styled(percent, bold().fg(accent()))];
    detail_spans.extend(detail.spans);
    lines.push(Line::from(detail_spans));
    frame.render_widget(Paragraph::new(lines), inner);
    buttons(
        frame,
        inner,
        &[
            ("Hide", Button::Close, app.choice == 0),
            ("Stop", Button::Stop, app.choice == 1),
        ],
        hits,
    );
}

/// The transfers under way, each with its progress; the keys pick one to stop.
fn draw_transfers(app: &App, row: usize, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let shown = app
        .transfers
        .len()
        .min(usize::from(area.height.saturating_sub(6)) / 4)
        .max(1);
    let inner = dialog(frame, area, 72, cells(shown * 4 + 3), "Under way", hits);
    let first = row.saturating_sub(shown - 1);
    for (index, transfer) in app.transfers.iter().enumerate().skip(first).take(shown) {
        let y = inner.y + cells((index - first) * 4);
        let focused = index == row;
        let pointer = if focused { "› " } else { "  " };
        let title_style = if focused { bold().fg(accent()) } else { bold() };
        frame.render_widget(
            Line::styled(
                format!(
                    "{pointer}{}",
                    fit(&transfer.title, usize::from(inner.width) - 2)
                ),
                title_style,
            ),
            Rect::new(inner.x, y, inner.width, 1),
        );
        let [bar, detail] = transfer_lines(transfer, usize::from(inner.width).saturating_sub(2));
        frame.render_widget(
            Paragraph::new(vec![bar, detail]),
            Rect::new(inner.x + 2, y + 1, inner.width.saturating_sub(2), 2),
        );
    }
    frame.render_widget(
        Line::styled(
            "↑↓ picks · Del stops it · Esc hides; they keep going",
            dim(),
        ),
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 1),
    );
    buttons(
        frame,
        inner,
        &[
            ("Stop", Button::Stop, app.choice == 0),
            ("Hide", Button::Close, app.choice == 1),
        ],
        hits,
    );
}

fn draw_help(app: &App, scroll: usize, frame: &mut Frame<'_>, area: Rect, hits: &mut Hits) {
    let mut lines: Vec<Line<'_>> = ticket(logo::FLAT).into_iter().map(Line::centered).collect();
    lines.push(Line::default());
    lines.push(
        Line::from(vec![
            Span::styled("tess ", bold().fg(accent())),
            Span::styled(env!("CARGO_PKG_VERSION"), dim()),
        ])
        .centered(),
    );
    lines.push(Line::default());
    for action in Action::ALL {
        let keys: Vec<String> = app.keymap.keys(action).map(|key| key.to_string()).collect();
        if keys.is_empty() {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(format!("{:<20}", keys.join(" ")), bold()),
            Span::raw(action.describe()),
            Span::styled(format!("  {}", action.name()), dim()),
        ]));
    }
    lines.push(Line::default());
    for (what, does) in [
        ("Drag", "Select text; letting go copies it"),
        ("Click", "Pick a row; on its box, mark it"),
        ("Ctrl-click", "Mark a row"),
        ("Double-click", "Open a folder; on a file, send"),
        ("Right-click", "What can be done there"),
        ("Paste a link", "Download it into this folder"),
    ] {
        lines.push(Line::from(vec![
            Span::styled(format!("{what:<20}"), bold()),
            Span::raw(does),
        ]));
    }
    lines.push(Line::default());
    let config = crate::config::default_path()
        .map(|path| tilde(&path))
        .unwrap_or_default();
    lines.push(Line::styled(
        "Change keys by the names on the right, under [ui.bindings] in",
        dim(),
    ));
    lines.push(Line::styled(config, dim()));
    let height = cells(lines.len() + 4).min(area.height.saturating_sub(2));
    let inner = dialog(frame, area, 76, height, "Keys", hits);
    let visible = usize::from(inner.height.saturating_sub(1));
    let scroll = scroll.min(lines.len().saturating_sub(visible));
    frame.render_widget(
        Paragraph::new(lines).scroll((cells(scroll), 0)),
        Rect {
            height: inner.height.saturating_sub(1),
            ..inner
        },
    );
    buttons(frame, inner, &[("Close", Button::Close, true)], hits);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_names_are_cut_where_they_stop_mattering() {
        assert_eq!(fit("report.pdf", 20), "report.pdf");
        assert_eq!(fit("quarterly-report.pdf", 10), "quarterly…");
        assert_eq!(fit("報告書.pdf", 5), "報告…");
        assert_eq!(fit_left("/home/me/projects/tessera", 12), "…cts/tessera");
    }

    #[test]
    fn an_expiry_reads_as_the_local_clock_does() {
        let taipei = time::UtcOffset::from_hms(8, 0, 0).expect("an offset");
        assert_eq!(
            when(Some("2026-10-26T01:56:00Z"), taipei),
            "2026-10-26 09:56"
        );
        assert_eq!(
            when(Some("2026-10-25T23:30:00Z"), taipei),
            "2026-10-26 07:30"
        );
        assert_eq!(when(None, taipei), "");
    }

    #[test]
    fn the_list_scrolls_only_as_far_as_the_cursor_needs() {
        assert_eq!(scrolled(0, 3, 10), 0);
        assert_eq!(scrolled(0, 12, 10), 3);
        assert_eq!(scrolled(5, 2, 10), 2);
        assert_eq!(scrolled(5, 9, 10), 5);
    }
}
