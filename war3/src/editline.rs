//! A line typed into an edit box and its cursor (CEditBox's text and cursor): typing
//! goes in at the cursor, Backspace and Delete take the character before and after it,
//! Left, Right, Home, and End move it, and a click puts it at the nearest gap.

use std::fmt;
use std::ops::Deref;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditLine {
    text: String,
    /// A byte offset into `text`, always at a character's start or the end.
    caret: usize,
}

/// The keys that move the cursor or delete after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKey {
    Left,
    Right,
    Home,
    End,
    Delete,
}

impl EditLine {
    /// The text, the cursor at its end.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        EditLine { caret: text.len(), text }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Replaces the text, the cursor going to its end.
    pub fn set(&mut self, text: impl Into<String>) {
        *self = EditLine::new(text);
    }

    pub fn clear(&mut self) {
        *self = EditLine::default();
    }

    /// The text, leaving the line empty.
    pub fn take(&mut self) -> String {
        std::mem::take(self).text
    }

    /// A character at the cursor, which moves past it.
    pub fn insert(&mut self, c: char) {
        self.text.insert(self.caret, c);
        self.caret += c.len_utf8();
    }

    /// Backspace: the character before the cursor.
    pub fn backspace(&mut self) {
        if let Some(c) = self.text[..self.caret].chars().next_back() {
            self.caret -= c.len_utf8();
            self.text.remove(self.caret);
        }
    }

    pub fn key(&mut self, key: EditKey) {
        match key {
            EditKey::Left => {
                if let Some(c) = self.text[..self.caret].chars().next_back() {
                    self.caret -= c.len_utf8();
                }
            }
            EditKey::Right => {
                if let Some(c) = self.text[self.caret..].chars().next() {
                    self.caret += c.len_utf8();
                }
            }
            EditKey::Home => self.caret = 0,
            EditKey::End => self.caret = self.text.len(),
            EditKey::Delete => {
                if self.caret < self.text.len() {
                    self.text.remove(self.caret);
                }
            }
        }
    }

    /// Puts the cursor at a byte offset (a click's, from FrameDb::caret_index), held to
    /// the text and to a character's start.
    pub fn set_caret(&mut self, at: usize) {
        let mut at = at.min(self.text.len());
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        self.caret = at;
    }
}

impl Deref for EditLine {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for EditLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_goes_in_at_the_cursor() {
        let mut l = EditLine::new("hello");
        l.set_caret(2);
        l.insert('X');
        assert_eq!((l.as_str(), l.caret()), ("heXllo", 3));
        l.backspace();
        l.key(EditKey::Delete);
        assert_eq!((l.as_str(), l.caret()), ("helo", 2));
        l.key(EditKey::Home);
        l.backspace();
        l.key(EditKey::Left);
        assert_eq!(l.caret(), 0);
        l.key(EditKey::End);
        l.key(EditKey::Right);
        l.key(EditKey::Delete);
        assert_eq!((l.as_str(), l.caret()), ("helo", 4));
    }

    #[test]
    fn the_cursor_keeps_to_whole_characters() {
        let mut l = EditLine::new("aé");
        l.set_caret(2);
        assert_eq!(l.caret(), 1);
        l.key(EditKey::Right);
        assert_eq!(l.caret(), 3);
        l.key(EditKey::Left);
        l.key(EditKey::Delete);
        assert_eq!(l.as_str(), "a");
    }
}
