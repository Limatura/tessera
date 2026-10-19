//! A one-line text field.

use crossterm::event::KeyCode;

/// The text typed so far, and where the caret is, in characters.
#[derive(Debug, Clone, Default)]
pub(crate) struct Input {
    text: String,
    caret: usize,
    /// Shown as dots: a password.
    pub(crate) masked: bool,
}

impl Input {
    pub(crate) fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            caret: text.chars().count(),
            masked: false,
        }
    }

    pub(crate) fn masked() -> Self {
        Self {
            masked: true,
            ..Self::default()
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// What to draw: the text, or a dot for each character of a password.
    pub(crate) fn shown(&self) -> String {
        if self.masked {
            "•".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }

    /// The caret, in characters from the start.
    pub(crate) const fn caret(&self) -> usize {
        self.caret
    }

    pub(crate) fn insert(&mut self, typed: char) {
        let at = self.byte(self.caret);
        self.text.insert(at, typed);
        self.caret += 1;
    }

    /// Pastes `text`, up to its first line break: a field holds one line.
    pub(crate) fn paste(&mut self, text: &str) {
        for typed in text.chars().take_while(|c| *c != '\n' && *c != '\r') {
            if !typed.is_control() {
                self.insert(typed);
            }
        }
    }

    /// Edits for a key that edits; says whether it was one.
    pub(crate) fn edit(&mut self, code: KeyCode) -> bool {
        let length = self.text.chars().count();
        match code {
            KeyCode::Backspace if self.caret > 0 => {
                self.caret -= 1;
                let at = self.byte(self.caret);
                self.text.remove(at);
            }
            KeyCode::Delete if self.caret < length => {
                let at = self.byte(self.caret);
                self.text.remove(at);
            }
            KeyCode::Left => self.caret = self.caret.saturating_sub(1),
            KeyCode::Right => self.caret = (self.caret + 1).min(length),
            KeyCode::Home => self.caret = 0,
            KeyCode::End => self.caret = length,
            KeyCode::Backspace | KeyCode::Delete => {}
            _ => return false,
        }
        true
    }

    fn byte(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(at, _)| at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_counts_characters_not_bytes() {
        let mut input = Input::new("報告");
        input.insert('書');
        assert_eq!(input.text(), "報告書");
        input.edit(KeyCode::Left);
        input.edit(KeyCode::Backspace);
        assert_eq!(input.text(), "報書");
        input.edit(KeyCode::Home);
        input.edit(KeyCode::Delete);
        assert_eq!(input.text(), "書");
        input.edit(KeyCode::Backspace);
        assert_eq!(input.text(), "書");
    }

    #[test]
    fn a_paste_keeps_its_first_line() {
        let mut input = Input::default();
        input.paste("https://share.example/s/AB12\nmore");
        assert_eq!(input.text(), "https://share.example/s/AB12");
        assert_eq!(input.caret(), 28);
    }

    #[test]
    fn a_password_shows_as_dots() {
        let mut input = Input::masked();
        input.paste("hunter2");
        assert_eq!(input.shown(), "•••••••");
    }
}
