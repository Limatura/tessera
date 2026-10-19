//! Which key does what: a preset's keys, with the config file's on top.

use std::{collections::BTreeMap, fmt};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    Error, Result,
    config::{Keys, Preset},
};

/// Everything a key can be bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Action {
    Up,
    Down,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    Top,
    Bottom,
    Open,
    Into,
    Parent,
    Mark,
    MarkAll,
    Filter,
    Find,
    GoTo,
    Send,
    Get,
    Copy,
    Delete,
    Refresh,
    ToggleHidden,
    ToggleMouse,
    NextTab,
    PreviousTab,
    Help,
    Settings,
    Quit,
}

impl Action {
    pub(crate) const ALL: [Self; 28] = [
        Self::Up,
        Self::Down,
        Self::PageUp,
        Self::PageDown,
        Self::HalfPageUp,
        Self::HalfPageDown,
        Self::Top,
        Self::Bottom,
        Self::Open,
        Self::Into,
        Self::Parent,
        Self::Mark,
        Self::MarkAll,
        Self::Filter,
        Self::Find,
        Self::GoTo,
        Self::Send,
        Self::Get,
        Self::Copy,
        Self::Delete,
        Self::Refresh,
        Self::ToggleHidden,
        Self::ToggleMouse,
        Self::NextTab,
        Self::PreviousTab,
        Self::Help,
        Self::Settings,
        Self::Quit,
    ];

    /// Its name in the config file.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::HalfPageUp => "half_page_up",
            Self::HalfPageDown => "half_page_down",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Open => "open",
            Self::Into => "into",
            Self::Parent => "parent",
            Self::Mark => "mark",
            Self::MarkAll => "mark_all",
            Self::Filter => "filter",
            Self::Find => "find",
            Self::GoTo => "go_to",
            Self::Send => "send",
            Self::Get => "get",
            Self::Copy => "copy",
            Self::Delete => "delete",
            Self::Refresh => "refresh",
            Self::ToggleHidden => "toggle_hidden",
            Self::ToggleMouse => "toggle_mouse",
            Self::NextTab => "next_tab",
            Self::PreviousTab => "previous_tab",
            Self::Help => "help",
            Self::Settings => "settings",
            Self::Quit => "quit",
        }
    }

    /// What it does, for the help.
    pub(crate) const fn describe(self) -> &'static str {
        match self {
            Self::Up => "Move up",
            Self::Down => "Move down",
            Self::PageUp => "Page up",
            Self::PageDown => "Page down",
            Self::HalfPageUp => "Half a page up",
            Self::HalfPageDown => "Half a page down",
            Self::Top => "First item",
            Self::Bottom => "Last item",
            Self::Open => "Open the folder; on a file, send",
            Self::Into => "Into the folder",
            Self::Parent => "Up one folder",
            Self::Mark => "Mark or unmark",
            Self::MarkAll => "Mark everything listed",
            Self::Filter => "Filter this folder",
            Self::Find => "Find under this folder",
            Self::GoTo => "Go to a path",
            Self::Send => "Send what is marked",
            Self::Get => "Download a link here",
            Self::Copy => "Copy the path, or the share's link",
            Self::Delete => "Delete the share",
            Self::Refresh => "Reload",
            Self::ToggleHidden => "Show or hide dotfiles",
            Self::ToggleMouse => "Mouse on or off",
            Self::NextTab => "Next tab",
            Self::PreviousTab => "Previous tab",
            Self::Help => "Keys",
            Self::Settings => "Keys, mouse, colour",
            Self::Quit => "Quit",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.name() == name)
    }
}

/// A key as it is bound: a code and Ctrl or Alt. Shift is part of the
/// character (`G`), or of the code (`BackTab`), so it is not kept apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Key {
    const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// The key a terminal reported.
    pub(crate) fn of(event: &KeyEvent) -> Self {
        let modifiers = event.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT);
        let code = match event.code {
            // A terminal sends Ctrl-letter as the letter with Ctrl, in either
            // case depending on Shift; a binding names it in lower case.
            KeyCode::Char(letter) if modifiers.contains(KeyModifiers::CONTROL) => {
                KeyCode::Char(letter.to_ascii_lowercase())
            }
            code => code,
        };
        Self { code, modifiers }
    }

    /// A character typed with nothing held, which can go into a text field.
    pub(crate) const fn typed(self) -> Option<char> {
        match self.code {
            KeyCode::Char(typed) if self.modifiers.is_empty() => Some(typed),
            _ => None,
        }
    }

    /// Reads `ctrl-s`, `alt-x`, `G`, `space`, `f5`, `pagedown`...
    fn parse(text: &str) -> Option<Self> {
        let mut modifiers = KeyModifiers::NONE;
        let mut rest = text;
        loop {
            let lower = rest.to_ascii_lowercase();
            if let Some(after) = lower.strip_prefix("ctrl-") {
                modifiers |= KeyModifiers::CONTROL;
                rest = rest.get(rest.len() - after.len()..)?;
            } else if let Some(after) = lower.strip_prefix("alt-") {
                modifiers |= KeyModifiers::ALT;
                rest = rest.get(rest.len() - after.len()..)?;
            } else {
                break;
            }
        }
        let mut chars = rest.chars();
        let code = match (chars.next(), chars.next()) {
            (Some(single), None) if modifiers.contains(KeyModifiers::CONTROL) => {
                KeyCode::Char(single.to_ascii_lowercase())
            }
            (Some(single), None) => KeyCode::Char(single),
            _ => match rest.to_ascii_lowercase().as_str() {
                "up" => KeyCode::Up,
                "down" => KeyCode::Down,
                "left" => KeyCode::Left,
                "right" => KeyCode::Right,
                "enter" | "return" => KeyCode::Enter,
                "esc" | "escape" => KeyCode::Esc,
                "tab" => KeyCode::Tab,
                "backtab" | "shift-tab" => KeyCode::BackTab,
                "backspace" => KeyCode::Backspace,
                "delete" | "del" => KeyCode::Delete,
                "insert" | "ins" => KeyCode::Insert,
                "home" => KeyCode::Home,
                "end" => KeyCode::End,
                "pageup" | "page_up" => KeyCode::PageUp,
                "pagedown" | "page_down" => KeyCode::PageDown,
                "space" => KeyCode::Char(' '),
                function => KeyCode::F(function.strip_prefix('f')?.parse().ok()?),
            },
        };
        Some(Self { code, modifiers })
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            f.write_str("Ctrl-")?;
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            f.write_str("Alt-")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(letter) if self.modifiers.contains(KeyModifiers::CONTROL) => {
                write!(f, "{}", letter.to_ascii_uppercase())
            }
            KeyCode::Char(typed) => write!(f, "{typed}"),
            KeyCode::Up => f.write_str("↑"),
            KeyCode::Down => f.write_str("↓"),
            KeyCode::Left => f.write_str("←"),
            KeyCode::Right => f.write_str("→"),
            KeyCode::Enter => f.write_str("Enter"),
            KeyCode::Esc => f.write_str("Esc"),
            KeyCode::Tab => f.write_str("Tab"),
            KeyCode::BackTab => f.write_str("Shift-Tab"),
            KeyCode::Backspace => f.write_str("Backspace"),
            KeyCode::Delete => f.write_str("Delete"),
            KeyCode::Insert => f.write_str("Insert"),
            KeyCode::Home => f.write_str("Home"),
            KeyCode::End => f.write_str("End"),
            KeyCode::PageUp => f.write_str("PgUp"),
            KeyCode::PageDown => f.write_str("PgDn"),
            KeyCode::F(number) => write!(f, "F{number}"),
            other => write!(f, "{other:?}"),
        }
    }
}

/// Every binding in force.
#[derive(Debug, Clone)]
pub(crate) struct Keymap {
    bindings: Vec<(Key, Action)>,
    /// Whether a typed character that is bound to nothing filters the list.
    pub(crate) type_to_filter: bool,
}

impl Keymap {
    /// The preset's keys, with `overrides` replacing the keys of the actions
    /// they name.
    ///
    /// # Errors
    ///
    /// When an override names no action, or a key that does not read.
    pub(crate) fn new(preset: Preset, overrides: &BTreeMap<String, Keys>) -> Result<Self> {
        let mut bindings = defaults(preset);
        for (name, keys) in overrides {
            let action = Action::named(name).ok_or_else(|| {
                Error::Usage(format!(
                    "[ui.bindings]: no action called `{name}`; the actions are {}",
                    Action::ALL.map(Action::name).join(", ")
                ))
            })?;
            let keys = keys
                .each()
                .iter()
                .map(|text| {
                    Key::parse(text).ok_or_else(|| {
                        Error::Usage(format!(
                            "[ui.bindings]: `{text}` for `{name}` is not a key; write keys like \
                             `k`, `ctrl-s`, `alt-x`, `f5`, `space`, `pagedown`"
                        ))
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            // The action loses its old keys, and the keys their old actions.
            bindings.retain(|(key, bound)| *bound != action && !keys.contains(key));
            bindings.extend(keys.into_iter().map(|key| (key, action)));
        }
        Ok(Self {
            bindings,
            type_to_filter: preset == Preset::Standard,
        })
    }

    /// What `key` does, if anything.
    pub(crate) fn action(&self, key: Key) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(bound, _)| *bound == key)
            .map(|(_, action)| *action)
    }

    /// The keys for `action`, in the order they were bound.
    pub(crate) fn keys(&self, action: Action) -> impl Iterator<Item = Key> + '_ {
        self.bindings
            .iter()
            .filter(move |(_, bound)| *bound == action)
            .map(|(key, _)| *key)
    }

    /// The first key for `action`, written for a hint: `^S`, `F1`, `s`.
    pub(crate) fn hint(&self, action: Action) -> Option<String> {
        self.keys(action).next().map(|key| key.to_string())
    }
}

fn defaults(preset: Preset) -> Vec<(Key, Action)> {
    use Action as A;
    use KeyCode as C;

    const NONE: KeyModifiers = KeyModifiers::NONE;
    const CTRL: KeyModifiers = KeyModifiers::CONTROL;
    let plain = |code| Key::new(code, NONE);
    let ctrl = |letter| Key::new(C::Char(letter), CTRL);
    let letter = |letter| Key::new(C::Char(letter), NONE);

    // Both presets: what every list in a terminal answers to.
    let mut bindings = vec![
        (plain(C::Up), A::Up),
        (plain(C::Down), A::Down),
        (plain(C::PageUp), A::PageUp),
        (plain(C::PageDown), A::PageDown),
        (plain(C::Home), A::Top),
        (plain(C::End), A::Bottom),
        (plain(C::Enter), A::Open),
        (plain(C::Right), A::Into),
        (plain(C::Left), A::Parent),
        (plain(C::Backspace), A::Parent),
        (letter(' '), A::Mark),
        (plain(C::Delete), A::Delete),
        (plain(C::F(5)), A::Refresh),
        (plain(C::Tab), A::NextTab),
        (plain(C::BackTab), A::PreviousTab),
        (plain(C::F(1)), A::Help),
        (plain(C::F(3)), A::Settings),
        (ctrl('c'), A::Quit),
    ];
    match preset {
        Preset::Standard => bindings.extend([
            (ctrl('a'), A::MarkAll),
            (ctrl('f'), A::Find),
            (ctrl('p'), A::Find),
            (ctrl('l'), A::GoTo),
            (ctrl('s'), A::Send),
            (ctrl('g'), A::Get),
            (ctrl('y'), A::Copy),
            (ctrl('r'), A::Refresh),
            (ctrl('t'), A::ToggleHidden),
            (plain(C::F(2)), A::ToggleMouse),
            (ctrl('u'), A::HalfPageUp),
            (ctrl('d'), A::HalfPageDown),
            (ctrl('q'), A::Quit),
        ]),
        Preset::Vim => bindings.extend([
            (letter('k'), A::Up),
            (letter('j'), A::Down),
            (ctrl('b'), A::PageUp),
            (ctrl('f'), A::PageDown),
            (ctrl('u'), A::HalfPageUp),
            (ctrl('d'), A::HalfPageDown),
            (letter('g'), A::Top),
            (letter('G'), A::Bottom),
            (letter('l'), A::Into),
            (letter('h'), A::Parent),
            (letter('a'), A::MarkAll),
            (letter('/'), A::Filter),
            (letter('f'), A::Find),
            (letter(':'), A::GoTo),
            (letter('s'), A::Send),
            (letter('p'), A::Get),
            (letter('y'), A::Copy),
            (letter('d'), A::Delete),
            (letter('r'), A::Refresh),
            (letter('.'), A::ToggleHidden),
            (letter('m'), A::ToggleMouse),
            (letter('?'), A::Help),
            (letter('q'), A::Quit),
        ]),
    }
    bindings
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyEventKind;

    use super::*;

    fn pressed(code: KeyCode, modifiers: KeyModifiers) -> Key {
        Key::of(&KeyEvent::new_with_kind(
            code,
            modifiers,
            KeyEventKind::Press,
        ))
    }

    #[test]
    fn keys_read_the_way_people_write_them() {
        let read = |text| Key::parse(text).expect(text);
        assert_eq!(
            read("ctrl-s"),
            pressed(KeyCode::Char('s'), KeyModifiers::CONTROL)
        );
        assert_eq!(
            read("Ctrl-S"),
            pressed(KeyCode::Char('S'), KeyModifiers::CONTROL)
        );
        assert_eq!(read("G"), pressed(KeyCode::Char('G'), KeyModifiers::SHIFT));
        assert_eq!(
            read("space"),
            pressed(KeyCode::Char(' '), KeyModifiers::NONE)
        );
        assert_eq!(read("f5"), pressed(KeyCode::F(5), KeyModifiers::NONE));
        assert_eq!(
            read("shift-tab"),
            pressed(KeyCode::BackTab, KeyModifiers::SHIFT)
        );
        assert_eq!(
            read("alt-x"),
            pressed(KeyCode::Char('x'), KeyModifiers::ALT)
        );
        assert_eq!(read("-"), pressed(KeyCode::Char('-'), KeyModifiers::NONE));
        assert_eq!(Key::parse("ctrl-"), None);
        assert_eq!(Key::parse("hyper-x"), None);
        assert_eq!(Key::parse("f"), Some(read("f")));
    }

    #[test]
    fn a_key_says_its_own_name() {
        assert_eq!(Key::parse("ctrl-s").expect("a key").to_string(), "Ctrl-S");
        assert_eq!(Key::parse("pagedown").expect("a key").to_string(), "PgDn");
        assert_eq!(Key::parse("space").expect("a key").to_string(), "Space");
    }

    #[test]
    fn the_presets_differ_where_letters_are_concerned() {
        let standard = Keymap::new(Preset::Standard, &BTreeMap::new()).expect("a keymap");
        let vim = Keymap::new(Preset::Vim, &BTreeMap::new()).expect("a keymap");
        let j = pressed(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(standard.action(j), None);
        assert!(standard.type_to_filter);
        assert_eq!(vim.action(j), Some(Action::Down));
        assert!(!vim.type_to_filter);
        let down = pressed(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(standard.action(down), Some(Action::Down));
        assert_eq!(vim.action(down), Some(Action::Down));
    }

    #[test]
    fn an_override_replaces_the_actions_keys_and_takes_the_key() {
        let overrides = BTreeMap::from([
            ("send".to_owned(), Keys::One("ctrl-x".to_owned())),
            ("quit".to_owned(), Keys::Many(vec!["ctrl-s".to_owned()])),
        ]);
        let keymap = Keymap::new(Preset::Standard, &overrides).expect("a keymap");
        let ctrl = |letter| pressed(KeyCode::Char(letter), KeyModifiers::CONTROL);
        assert_eq!(keymap.action(ctrl('x')), Some(Action::Send));
        assert_eq!(keymap.action(ctrl('s')), Some(Action::Quit));
        assert_eq!(keymap.action(ctrl('q')), None);
        assert_eq!(keymap.hint(Action::Send).as_deref(), Some("Ctrl-X"));
    }

    #[test]
    fn a_wrong_override_says_what_is_wrong() {
        let unknown = BTreeMap::from([("launch".to_owned(), Keys::One("x".to_owned()))]);
        let error = Keymap::new(Preset::Standard, &unknown).expect_err("refused");
        assert!(
            error.to_string().contains("no action called `launch`"),
            "{error}"
        );
        let unreadable = BTreeMap::from([("send".to_owned(), Keys::One("hyper-s".to_owned()))]);
        let error = Keymap::new(Preset::Standard, &unreadable).expect_err("refused");
        assert!(error.to_string().contains("`hyper-s`"), "{error}");
    }

    #[test]
    fn every_action_has_a_unique_name() {
        let mut names: Vec<_> = Action::ALL.map(Action::name).to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Action::ALL.len());
    }
}
