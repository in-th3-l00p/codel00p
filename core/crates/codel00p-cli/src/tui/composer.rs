//! The message composer: an editable input buffer with a cursor. Pure state (no
//! terminal), so its editing behavior is unit-tested directly. The cursor is a
//! character index in `[0, char_count]`; multi-line input is supported via explicit
//! newlines (Alt/Shift+Enter in the UI).

#[derive(Clone, Debug, Default)]
pub(crate) struct Composer {
    text: String,
    /// Cursor position as a char index into `text`.
    cursor: usize,
}

impl Composer {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replaces the buffer and parks the cursor at the end. Used to seed the
    /// composer programmatically (e.g. pre-filling an edit with current content).
    pub(crate) fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.char_count();
    }

    pub(crate) fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Returns the trimmed buffer and clears the composer — the submit path.
    pub(crate) fn take(&mut self) -> String {
        let out = self.text.trim().to_string();
        self.clear();
        out
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        let at = self.byte_at(self.cursor);
        self.text.insert(at, ch);
        self.cursor += 1;
    }

    pub(crate) fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    /// Deletes the character before the cursor (Backspace).
    pub(crate) fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_at(self.cursor - 1);
        let end = self.byte_at(self.cursor);
        self.text.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// Deletes the character at the cursor (Delete).
    pub(crate) fn delete(&mut self) {
        if self.cursor >= self.char_count() {
            return;
        }
        let start = self.byte_at(self.cursor);
        let end = self.byte_at(self.cursor + 1);
        self.text.replace_range(start..end, "");
    }

    pub(crate) fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(crate) fn right(&mut self) {
        if self.cursor < self.char_count() {
            self.cursor += 1;
        }
    }

    /// Moves to the start of the current logical line.
    pub(crate) fn home(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1] != '\n' {
            i -= 1;
        }
        self.cursor = i;
    }

    /// Moves to the end of the current logical line.
    pub(crate) fn end(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor;
        while i < chars.len() && chars[i] != '\n' {
            i += 1;
        }
        self.cursor = i;
    }

    fn char_count(&self) -> usize {
        self.text.chars().count()
    }

    /// Byte offset of a character index (clamped to the buffer length).
    fn byte_at(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map(|(byte, _)| byte)
            .unwrap_or(self.text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::Composer;

    fn typed(text: &str) -> Composer {
        let mut composer = Composer::default();
        for ch in text.chars() {
            composer.insert_char(ch);
        }
        composer
    }

    #[test]
    fn inserts_at_the_cursor() {
        let mut composer = typed("helo");
        composer.left(); // between 'l' and 'o'
        composer.insert_char('l');
        assert_eq!(composer.text(), "hello");
        assert_eq!(composer.cursor(), 4);
    }

    #[test]
    fn backspace_and_delete_respect_the_cursor() {
        let mut composer = typed("abcd");
        composer.left();
        composer.left(); // between 'b' and 'c'
        composer.backspace(); // removes 'b'
        assert_eq!(composer.text(), "acd");
        composer.delete(); // removes 'c'
        assert_eq!(composer.text(), "ad");
    }

    #[test]
    fn home_and_end_move_within_the_logical_line() {
        let mut composer = typed("one\ntwo");
        composer.home();
        assert_eq!(composer.cursor(), 4); // start of "two"
        composer.end();
        assert_eq!(composer.cursor(), 7); // end of "two"
    }

    #[test]
    fn left_right_clamp_at_bounds() {
        let mut composer = typed("hi");
        composer.left();
        composer.left();
        composer.left();
        assert_eq!(composer.cursor(), 0);
        composer.right();
        composer.right();
        composer.right();
        assert_eq!(composer.cursor(), 2);
    }

    #[test]
    fn newline_inserts_and_take_trims_and_clears() {
        let mut composer = typed("  hello ");
        composer.insert_newline();
        composer.insert_char('x');
        assert_eq!(composer.text(), "  hello \nx");
        assert_eq!(composer.take(), "hello \nx");
        assert!(composer.is_empty());
        assert_eq!(composer.cursor(), 0);
    }

    #[test]
    fn handles_multibyte_characters() {
        let mut composer = typed("café");
        composer.backspace();
        assert_eq!(composer.text(), "caf");
        composer.insert_char('é');
        assert_eq!(composer.text(), "café");
    }
}
