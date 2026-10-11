//! The text an editing item holds: the string, the cursor and the other end
//! of the selection, and the undo history (ARCHITECTURE §7.5).
//!
//! Positions count UTF-16 code units, as `cursorPosition` does in QML and as
//! the platforms' input methods do. A position inside a surrogate pair is
//! moved to its end, so the string is never cut inside a character.
//!
//! What counts as a word, and where the cursor lands after an edit or an
//! undo, follows Qt 6.11 measured as a black box (`selectWord()`, `insert()`,
//! `remove()`, `undo()` on `TextInput` and `TextEdit`).

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// `TextInput.maximumLength` when nothing set it.
pub const DEFAULT_MAX_LENGTH: usize = 32767;

/// Byte offset of the UTF-16 position `pos` in `text`. A position past the
/// end is the end; one inside a surrogate pair is the end of the pair.
pub fn utf16_to_byte(text: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= pos {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// UTF-16 position of the byte offset `byte` in `text`. An offset inside a
/// character counts the whole character.
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return utf16_len(text);
    }
    text.char_indices()
        .take_while(|(b, _)| *b < byte)
        .map(|(_, ch)| ch.len_utf16())
        .sum()
}

/// Length of `text` in UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().map(char::len_utf16).sum()
    }
}

/// The start of `text` that is at most `units` UTF-16 code units long.
fn prefix_utf16(text: &str, units: usize) -> &str {
    let mut used = 0;
    for (byte, ch) in text.char_indices() {
        used += ch.len_utf16();
        if used > units {
            return &text[..byte];
        }
    }
    text
}

/// What a character is to word movement. A word is a run of one class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Punctuation,
}

fn class_of(ch: char) -> Class {
    if ch.is_whitespace() {
        Class::Space
    } else if ch.is_alphanumeric() || ch == '_' || is_mark(ch) {
        Class::Word
    } else {
        Class::Punctuation
    }
}

/// Combining marks stay with the letter they sit on.
fn is_mark(ch: char) -> bool {
    matches!(u32::from(ch),
        0x0300..=0x036F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x0610..=0x061A
        | 0x064B..=0x065F | 0x0900..=0x0903 | 0x093A..=0x094F | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF | 0x200C..=0x200D | 0x20D0..=0x20FF | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F)
}

/// One change to the string, as the undo history keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    /// Byte offset of the change.
    at: usize,
    removed: String,
    inserted: String,
    /// Where the cursor goes when this is undone (a byte offset).
    restore: usize,
    /// Edits of one step are undone together.
    step: u64,
}

/// A point to return to when an edit turns out not to be allowed (a
/// validator said no). Taken with [`TextModel::checkpoint`].
#[derive(Clone, Debug)]
pub struct Checkpoint {
    text: String,
    cursor: usize,
    anchor: usize,
    undo_len: usize,
    redo: Vec<Edit>,
    step: u64,
    break_next: bool,
    clean: Option<usize>,
}

/// The text of an editing item with its cursor, selection and undo history.
#[derive(Clone, Debug)]
pub struct TextModel {
    text: String,
    /// Byte offsets, always on character boundaries.
    cursor: usize,
    anchor: usize,
    max_length: usize,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    step: u64,
    /// The next edit starts a new undo step.
    break_next: bool,
    /// How many changes the history held when the text was last said to be
    /// unmodified; `None` once no undo or redo leads back there.
    clean: Option<usize>,
}

impl Default for TextModel {
    fn default() -> Self {
        Self::new()
    }
}

impl TextModel {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            anchor: 0,
            max_length: DEFAULT_MAX_LENGTH,
            undo: Vec::new(),
            redo: Vec::new(),
            step: 0,
            break_next: false,
            clean: Some(0),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Length in UTF-16 code units.
    pub fn len(&self) -> usize {
        utf16_len(&self.text)
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn byte(&self, pos: usize) -> usize {
        utf16_to_byte(&self.text, pos)
    }

    fn units(&self, byte: usize) -> usize {
        byte_to_utf16(&self.text, byte)
    }

    /// Where the cursor is.
    pub fn cursor(&self) -> usize {
        self.units(self.cursor)
    }

    /// The end of the selection the cursor is not at; the cursor's own
    /// position without a selection.
    pub fn anchor(&self) -> usize {
        self.units(self.anchor)
    }

    /// The cursor as a byte offset into [`Self::text`].
    pub fn cursor_byte(&self) -> usize {
        self.cursor
    }

    /// The selection as byte offsets into [`Self::text`].
    pub fn selection_bytes(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    /// The selection, start before end; empty at the cursor without one.
    pub fn selection(&self) -> Range<usize> {
        let r = self.selection_bytes();
        self.units(r.start)..self.units(r.end)
    }

    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection_bytes()]
    }

    /// The text between two positions, in either order, cut to the text.
    pub fn slice(&self, a: usize, b: usize) -> &str {
        let (a, b) = (self.byte(a.min(b)), self.byte(a.max(b)));
        &self.text[a..b]
    }

    pub fn max_length(&self) -> usize {
        self.max_length
    }

    /// Sets the longest text allowed, cutting the text if it is longer.
    /// Returns whether the text changed.
    pub fn set_max_length(&mut self, max: usize) -> bool {
        self.max_length = max;
        if self.len() <= max {
            return false;
        }
        let keep = prefix_utf16(&self.text, max).len();
        self.text.truncate(keep);
        self.cursor = self.cursor.min(keep);
        self.anchor = self.anchor.min(keep);
        self.clear_undo();
        true
    }

    /// Replaces the whole text, as assigning `text` does: the history is
    /// forgotten and the cursor goes to the end. Returns whether the text
    /// changed.
    pub fn set_text(&mut self, text: &str) -> bool {
        let text = prefix_utf16(text, self.max_length);
        if text == self.text {
            return false;
        }
        self.text.clear();
        self.text.push_str(text);
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self.clear_undo();
        // A text that is assigned is the text as it was last saved.
        self.clean = Some(0);
        true
    }

    /// Moves the cursor; with `select` the other end of the selection stays
    /// where it is.
    pub fn move_to(&mut self, pos: usize, select: bool) {
        self.cursor = self.byte(pos);
        if !select {
            self.anchor = self.cursor;
        }
    }

    /// Selects from `anchor` to `cursor` and leaves the cursor at `cursor`.
    pub fn select(&mut self, anchor: usize, cursor: usize) {
        self.anchor = self.byte(anchor);
        self.cursor = self.byte(cursor);
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    /// Drops the selection; the cursor stays.
    pub fn deselect(&mut self) {
        self.anchor = self.cursor;
    }

    // --- boundaries ---------------------------------------------------

    /// The position one character (grapheme cluster) after `pos`.
    pub fn next_grapheme(&self, pos: usize) -> usize {
        let at = self.byte(pos);
        let next = self.text[at..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(self.text.len(), |(i, _)| at + i);
        self.units(next)
    }

    /// The position one character (grapheme cluster) before `pos`.
    pub fn prev_grapheme(&self, pos: usize) -> usize {
        let at = self.byte(pos);
        let prev = self.text[..at]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i);
        self.units(prev)
    }

    /// Runs of one class, as byte ranges, in order.
    fn runs(&self) -> impl Iterator<Item = (Range<usize>, Class)> + '_ {
        let mut chars = self.text.char_indices().peekable();
        std::iter::from_fn(move || {
            let (start, first) = chars.next()?;
            let class = class_of(first);
            let mut end = start + first.len_utf8();
            // A line break ends a word of spaces too: words do not span
            // paragraphs.
            if first != '\n' {
                while let Some((i, ch)) = chars.peek().copied() {
                    if ch == '\n' || class_of(ch) != class {
                        break;
                    }
                    end = i + ch.len_utf8();
                    chars.next();
                }
            }
            Some((start..end, class))
        })
    }

    /// Byte ranges of the words: the runs that are not white space.
    fn words(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.runs()
            .filter(|(_, class)| *class != Class::Space)
            .map(|(range, _)| range)
    }

    /// The start of the next word after `pos`; the end of the text when
    /// there is none (Ctrl+Right). The end of a paragraph is a stop too.
    pub fn next_word(&self, pos: usize) -> usize {
        let at = self.byte(pos);
        let word = self
            .words()
            .map(|w| w.start)
            .find(|s| *s > at)
            .unwrap_or(self.text.len());
        let line_end = self.text[at..]
            .char_indices()
            .skip(1)
            .find(|(_, ch)| *ch == '\n')
            .map_or(word, |(i, _)| at + i);
        self.units(word.min(line_end))
    }

    /// The start of the word before `pos`; the start of the text when there
    /// is none (Ctrl+Left). The end of a paragraph is a stop too.
    pub fn prev_word(&self, pos: usize) -> usize {
        let at = self.byte(pos);
        let word = self
            .words()
            .map(|w| w.start)
            .take_while(|s| *s < at)
            .last()
            .unwrap_or(0);
        let line_end = self.text[..at].rfind('\n').unwrap_or(0);
        self.units(word.max(line_end))
    }

    /// The end of the word at or after `pos`.
    pub fn word_end(&self, pos: usize) -> usize {
        let at = self.byte(pos);
        let end = self
            .words()
            .map(|w| w.end)
            .find(|e| *e > at)
            .unwrap_or(self.text.len());
        self.units(end)
    }

    fn word_bytes(&self, at: usize, reach_cursor: bool) -> Range<usize> {
        let paragraph = self.paragraph_bytes(at);
        let mut found: Option<Range<usize>> = None;
        for w in self.words() {
            if w.start > at {
                break;
            }
            if w.start >= paragraph.start {
                found = Some(w);
            }
        }
        match found {
            Some(w) if reach_cursor => w.start..w.end.max(at),
            // The cursor is in the white space after the last word of a
            // text: there is no word there.
            Some(w) if at == self.text.len() && w.end < at => at..at,
            Some(w) => w,
            None if reach_cursor => paragraph.start..at,
            None => at..at,
        }
    }

    /// The word `selectWord()` selects with the cursor at `pos`: the word
    /// that starts there, else the one before. With `reach_cursor` (a
    /// one-line field) the selection also covers the spaces between that
    /// word and the cursor.
    pub fn word_at(&self, pos: usize, reach_cursor: bool) -> Range<usize> {
        let r = self.word_bytes(self.byte(pos), reach_cursor);
        self.units(r.start)..self.units(r.end)
    }

    /// Selects the word at the cursor and leaves the cursor at its end.
    pub fn select_word(&mut self, reach_cursor: bool) {
        let r = self.word_bytes(self.cursor, reach_cursor);
        self.anchor = r.start;
        self.cursor = r.end;
    }

    fn paragraph_bytes(&self, at: usize) -> Range<usize> {
        let start = self.text[..at].rfind('\n').map_or(0, |i| i + 1);
        let end = self.text[at..]
            .find('\n')
            .map_or(self.text.len(), |i| at + i);
        start..end
    }

    /// The start of the paragraph `pos` is in (after the line break before it).
    pub fn paragraph_start(&self, pos: usize) -> usize {
        self.units(self.paragraph_bytes(self.byte(pos)).start)
    }

    /// The end of the paragraph `pos` is in (before its line break).
    pub fn paragraph_end(&self, pos: usize) -> usize {
        self.units(self.paragraph_bytes(self.byte(pos)).end)
    }

    /// `moveCursorSelection(pos, mode)`: moves the cursor to `pos` and keeps
    /// the other end. With `words` both ends go out to word boundaries.
    pub fn move_cursor_selection(&mut self, pos: usize, words: bool) {
        let to = self.byte(pos);
        if !words {
            self.cursor = to;
            return;
        }
        let anchor = self.anchor;
        let word_start = |at: usize| {
            self.words()
                .take_while(|w| w.start <= at)
                .last()
                .filter(|w| w.end > at || w.start == at)
                .map_or(at, |w| w.start)
        };
        let word_end = |at: usize| {
            self.words()
                .find(|w| w.end >= at)
                .filter(|w| w.start < at)
                .map_or(at, |w| w.end)
        };
        let (anchor, cursor) = if to >= anchor {
            (word_start(anchor), word_end(to))
        } else {
            (word_end(anchor), word_start(to))
        };
        self.anchor = anchor;
        self.cursor = cursor;
    }

    // --- editing ------------------------------------------------------

    /// How many more UTF-16 units fit once `removed` bytes at `at` are gone.
    fn room(&self, removed: &Range<usize>) -> usize {
        let after = self.len() - utf16_len(&self.text[removed.clone()]);
        self.max_length.saturating_sub(after)
    }

    /// Replaces `range` (bytes) by `text` and records it. Undoing it puts
    /// the cursor at `restore`.
    fn apply(&mut self, range: Range<usize>, text: &str, restore: usize) {
        if range.is_empty() && text.is_empty() {
            return;
        }
        if self.break_next {
            self.step += 1;
            self.break_next = false;
        }
        if self.clean.is_some_and(|clean| clean > self.undo.len()) {
            // The unmodified text was a redo away, and this edit forgets
            // what could be redone.
            self.clean = None;
        }
        self.undo.push(Edit {
            at: range.start,
            removed: self.text[range.clone()].to_owned(),
            inserted: text.to_owned(),
            restore,
            step: self.step,
        });
        self.redo.clear();
        self.text.replace_range(range, text);
    }

    /// `insert(pos, text)` from a program: the cursor and the selection
    /// move with the text around them. Text beyond the maximum length is
    /// dropped. Returns whether anything was inserted.
    pub fn insert(&mut self, pos: usize, text: &str) -> bool {
        let at = self.byte(pos);
        let text = prefix_utf16(text, self.room(&(at..at)));
        if text.is_empty() {
            return false;
        }
        self.apply(at..at, text, at);
        for p in [&mut self.cursor, &mut self.anchor] {
            if *p >= at {
                *p += text.len();
            }
        }
        true
    }

    /// `remove(start, end)` from a program, positions in either order. The
    /// cursor and the selection move with the text around them. Returns
    /// whether anything was removed.
    pub fn remove(&mut self, start: usize, end: usize) -> bool {
        let (a, b) = (self.byte(start.min(end)), self.byte(start.max(end)));
        if a == b {
            return false;
        }
        self.apply(a..b, "", b);
        for p in [&mut self.cursor, &mut self.anchor] {
            if *p >= b {
                *p -= b - a;
            } else if *p > a {
                *p = a;
            }
        }
        true
    }

    /// Puts `text` in place of the range (UTF-16 positions) and leaves the
    /// cursor after it: typing, pasting, and what an input method commits.
    /// Text beyond the maximum length is dropped. Returns whether the text
    /// changed.
    ///
    /// Undoing it puts the cursor back where it was before, when that was
    /// at the range (Backspace returns to the right of the character,
    /// Delete to the left, typing over a selection to the end the cursor
    /// was at).
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> bool {
        let (a, b) = (
            self.byte(range.start.min(range.end)),
            self.byte(range.start.max(range.end)),
        );
        let text = prefix_utf16(text, self.room(&(a..b)));
        let changed = a != b || !text.is_empty();
        let restore = if (a..=b).contains(&self.cursor) {
            self.cursor
        } else {
            b
        };
        self.apply(a..b, text, restore);
        self.cursor = a + text.len();
        self.anchor = self.cursor;
        changed
    }

    /// Puts `text` in place of the selection, or at the cursor without one.
    pub fn replace_selection(&mut self, text: &str) -> bool {
        self.replace(self.selection(), text)
    }

    /// Types `text` over what follows the cursor (`overwriteMode`): each
    /// character replaces one, but never a line break.
    pub fn overwrite(&mut self, text: &str) -> bool {
        if self.has_selection() {
            return self.replace_selection(text);
        }
        let count = text.graphemes(true).count();
        let end = self.text[self.cursor..]
            .grapheme_indices(true)
            .take_while(|(_, g)| !g.starts_with('\n'))
            .nth(count.saturating_sub(1))
            .map_or_else(
                || self.paragraph_bytes(self.cursor).end,
                |(i, g)| self.cursor + i + g.len(),
            );
        let range = self.units(self.cursor)..self.units(end.max(self.cursor));
        self.replace(range, text)
    }

    /// Removes the selection. Returns whether there was one.
    pub fn delete_selection(&mut self) -> bool {
        self.has_selection() && self.replace_selection("")
    }

    /// Backspace: the selection, else the code point before the cursor (so
    /// an accent typed after a letter goes before the letter does).
    pub fn backspace(&mut self) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        let Some(ch) = self.text[..self.cursor].chars().next_back() else {
            return false;
        };
        let start = self.cursor - ch.len_utf8();
        let range = self.units(start)..self.units(self.cursor);
        self.replace(range, "")
    }

    /// Delete: the selection, else the character after the cursor.
    pub fn delete_forward(&mut self) -> bool {
        if self.has_selection() {
            return self.delete_selection();
        }
        let cursor = self.cursor();
        let next = self.next_grapheme(cursor);
        next != cursor && self.replace(cursor..next, "")
    }

    // --- undo ---------------------------------------------------------

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Makes the next edit the start of a new undo step. Edits without a
    /// break between them are undone together (a run of typing).
    pub fn break_undo(&mut self) {
        self.break_next = true;
    }

    pub fn clear_undo(&mut self) {
        // What the text is does not change, so neither does whether it is
        // modified.
        self.clean = (!self.is_modified()).then_some(0);
        self.undo.clear();
        self.redo.clear();
        self.break_next = false;
    }

    /// Whether the text differs from what it was when it was assigned or
    /// last said to be unmodified. Undoing or redoing back to that text
    /// makes it unmodified again.
    pub fn is_modified(&self) -> bool {
        self.clean != Some(self.undo.len())
    }

    /// Says the text as it is now is unmodified (a program saved it), or
    /// that it is modified whatever the history says.
    pub fn set_modified(&mut self, modified: bool) {
        if !modified {
            self.clean = Some(self.undo.len());
            // What is typed next is a change to the saved text, so it must
            // not join the step that led here.
            self.break_next = true;
        } else if !self.is_modified() {
            self.clean = None;
        }
    }

    /// Takes back the last step. The cursor goes to where it was before the
    /// first change of the step.
    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.last().map(|e| e.step) else {
            return false;
        };
        while let Some(edit) = self.undo.pop_if(|e| e.step == step) {
            let end = edit.at + edit.inserted.len();
            self.text.replace_range(edit.at..end, &edit.removed);
            self.cursor = edit.restore;
            self.redo.push(edit);
        }
        self.anchor = self.cursor;
        self.break_next = true;
        true
    }

    /// Does again what the last undo took back.
    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.last().map(|e| e.step) else {
            return false;
        };
        while let Some(edit) = self.redo.pop_if(|e| e.step == step) {
            let end = edit.at + edit.removed.len();
            self.text.replace_range(edit.at..end, &edit.inserted);
            self.cursor = edit.at + edit.inserted.len();
            self.undo.push(edit);
        }
        self.anchor = self.cursor;
        self.break_next = true;
        true
    }

    /// The state now, to come back to with [`Self::rollback`].
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
            undo_len: self.undo.len(),
            redo: self.redo.clone(),
            step: self.step,
            break_next: self.break_next,
            clean: self.clean,
        }
    }

    /// Returns to a checkpoint, as if the edits since had not happened.
    pub fn rollback(&mut self, to: Checkpoint) {
        self.text = to.text;
        self.cursor = to.cursor;
        self.anchor = to.anchor;
        self.undo.truncate(to.undo_len);
        self.redo = to.redo;
        self.step = to.step;
        self.break_next = to.break_next;
        self.clean = to.clean;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(text: &str) -> TextModel {
        let mut m = TextModel::new();
        m.set_text(text);
        m
    }

    #[test]
    fn modified_follows_the_history_back_to_the_saved_text() {
        let mut m = model("ab");
        assert!(!m.is_modified());
        m.insert(2, "c");
        assert!(m.is_modified());
        m.undo();
        assert!(!m.is_modified());
        m.redo();
        m.set_modified(false);
        assert!(!m.is_modified());
        m.undo();
        assert!(m.is_modified());
        m.redo();
        assert!(!m.is_modified());
        // An edit made where the saved text was a redo away loses it.
        m.undo();
        m.insert(0, "x");
        m.undo();
        assert!(m.is_modified());
        m.redo();
        assert!(m.is_modified());
        m.set_text("new");
        assert!(!m.is_modified());
        m.set_modified(true);
        assert!(m.is_modified());
    }

    #[test]
    fn typing_after_a_save_is_a_step_of_its_own() {
        let mut m = model("");
        m.insert(0, "a");
        m.set_modified(false);
        m.insert(1, "b");
        assert!(m.is_modified());
        m.undo();
        assert_eq!(m.text(), "a");
        assert!(!m.is_modified());
    }

    #[test]
    fn utf16_positions_convert_both_ways() {
        let s = "a😀b";
        assert_eq!(utf16_len(s), 4);
        assert_eq!(utf16_to_byte(s, 0), 0);
        assert_eq!(utf16_to_byte(s, 1), 1);
        // Inside the pair: its end.
        assert_eq!(utf16_to_byte(s, 2), 5);
        assert_eq!(utf16_to_byte(s, 3), 5);
        assert_eq!(utf16_to_byte(s, 4), 6);
        assert_eq!(utf16_to_byte(s, 99), 6);
        assert_eq!(byte_to_utf16(s, 5), 3);
        assert_eq!(byte_to_utf16(s, 6), 4);
        assert_eq!(byte_to_utf16(s, 99), 4);
    }

    #[test]
    fn setting_the_text_puts_the_cursor_at_the_end() {
        let m = model("hello");
        assert_eq!((m.cursor(), m.anchor()), (5, 5));
        assert!(!m.can_undo());
    }

    #[test]
    fn select_leaves_the_cursor_at_the_second_position() {
        let mut m = model("hello brave world");
        m.select(2, 7);
        assert_eq!(
            (m.cursor(), m.selection(), m.selected_text()),
            (7, 2..7, "llo b")
        );
        m.select(7, 2);
        assert_eq!((m.cursor(), m.selection()), (2, 2..7));
        m.deselect();
        assert_eq!(m.selection(), 2..2);
        m.select_all();
        assert_eq!((m.cursor(), m.selection()), (17, 0..17));
    }

    #[test]
    fn a_programmatic_insert_moves_what_is_after_it() {
        let mut m = model("hello brave world");
        m.move_to(3, false);
        assert!(m.insert(0, "X"));
        assert_eq!((m.text(), m.cursor()), ("Xhello brave world", 4));
        m.set_text("hello brave world");
        m.select(2, 7);
        m.insert(3, "Z");
        assert_eq!(
            (m.text(), m.selection(), m.cursor()),
            ("helZlo brave world", 2..8, 8)
        );
        // At the start of the selection the whole selection moves.
        m.set_text("hello brave world");
        m.select(2, 7);
        m.insert(2, "X");
        assert_eq!(m.selection(), 3..8);
        // At the cursor the cursor ends up after the text.
        m.set_text("hello");
        m.insert(5, "X");
        assert_eq!(m.cursor(), 6);
    }

    #[test]
    fn a_programmatic_remove_moves_what_is_after_it() {
        let mut m = model("Xhello brave world");
        m.move_to(4, false);
        assert!(m.remove(0, 1));
        assert_eq!((m.text(), m.cursor()), ("hello brave world", 3));
        // Either order.
        assert!(m.remove(5, 2));
        assert_eq!((m.text(), m.cursor()), ("he brave world", 2));
        assert!(!m.remove(3, 3));
        // Past the end is the end.
        assert!(m.remove(2, 100));
        assert_eq!(m.text(), "he");
    }

    #[test]
    fn edits_without_a_break_are_one_undo_step() {
        let mut m = model("hello");
        m.insert(5, "1");
        m.insert(6, "2");
        assert!(m.undo());
        assert_eq!((m.text(), m.cursor()), ("hello", 5));
        assert!(!m.can_undo() && m.can_redo());
        assert!(!m.undo());
        assert!(m.redo());
        assert_eq!((m.text(), m.cursor()), ("hello12", 7));
        assert!(!m.redo());
    }

    #[test]
    fn a_break_starts_a_new_undo_step() {
        let mut m = model("hello");
        m.insert(5, "1");
        m.break_undo();
        m.insert(6, "2");
        m.undo();
        assert_eq!((m.text(), m.cursor()), ("hello1", 6));
        assert!(m.can_undo() && m.can_redo());
        m.undo();
        assert_eq!(m.text(), "hello");
    }

    #[test]
    fn undo_puts_the_cursor_where_the_change_was() {
        let mut m = model("hello brave world");
        m.move_to(3, false);
        m.insert(0, "X");
        m.remove(0, 1);
        m.remove(5, 2);
        assert_eq!(m.text(), "he brave world");
        m.undo();
        assert_eq!(
            (m.text(), m.cursor(), m.selection()),
            ("hello brave world", 0, 0..0)
        );
        m.redo();
        assert_eq!((m.text(), m.cursor()), ("he brave world", 2));
    }

    #[test]
    fn undo_puts_the_cursor_back_where_the_user_had_it() {
        // Measured on Qt 6.11 with keys: Delete twice at the start.
        let mut m = model("hello");
        m.move_to(0, false);
        m.delete_forward();
        m.delete_forward();
        m.undo();
        assert_eq!((m.text(), m.cursor()), ("hello", 0));
        // Backspace twice at the end.
        let mut m = model("hello");
        m.backspace();
        m.backspace();
        m.undo();
        assert_eq!((m.text(), m.cursor()), ("hello", 5));
        // Typing over a selection made leftwards from the end.
        let mut m = model("hello");
        m.select(5, 3);
        m.replace_selection("x");
        m.replace_selection("y");
        assert_eq!(m.text(), "helxy");
        m.undo();
        assert_eq!(
            (m.text(), m.cursor(), m.has_selection()),
            ("hello", 3, false)
        );
        // Typing over everything.
        let mut m = model("abc");
        m.select_all();
        m.replace_selection("x");
        m.undo();
        assert_eq!((m.text(), m.cursor()), ("abc", 3));
    }

    #[test]
    fn a_new_edit_forgets_what_could_be_redone() {
        let mut m = model("hello");
        m.insert(5, "1");
        m.undo();
        m.insert(0, "Z");
        assert!(!m.can_redo());
        assert_eq!(m.text(), "Zhello");
    }

    #[test]
    fn typing_replaces_the_selection() {
        let mut m = model("hello brave world");
        m.select(2, 7);
        assert!(m.replace_selection("X"));
        assert_eq!(
            (m.text(), m.cursor(), m.has_selection()),
            ("heXrave world", 3, false)
        );
        m.undo();
        assert_eq!(m.text(), "hello brave world");
    }

    #[test]
    fn the_maximum_length_cuts_what_is_set_and_what_is_typed() {
        let mut m = TextModel::new();
        m.set_max_length(5);
        m.set_text("abcdefghij");
        assert_eq!((m.text(), m.cursor()), ("abcde", 5));
        assert!(!m.insert(0, "ZZ"));
        m.set_text("ab");
        m.insert(2, "12345");
        assert_eq!(m.text(), "ab123");
        // Replacing makes room.
        m.select(0, 2);
        m.replace_selection("wxyz");
        assert_eq!(m.text(), "wx123");
        // A pair that does not fit whole is left out.
        m.set_text("abcd");
        m.replace_selection("😀");
        assert_eq!(m.text(), "abcd");
        assert!(m.set_max_length(2));
        assert_eq!((m.text(), m.cursor()), ("ab", 2));
    }

    #[test]
    fn backspace_takes_a_code_point_and_delete_a_character() {
        let mut m = model("e\u{301}x");
        m.move_to(2, false);
        assert!(m.backspace());
        assert_eq!(m.text(), "ex");
        let mut m = model("e\u{301}x");
        m.move_to(0, false);
        assert!(m.delete_forward());
        assert_eq!(m.text(), "x");
        assert!(m.delete_forward());
        assert!(!m.delete_forward());
        assert!(!m.backspace());
    }

    #[test]
    fn the_cursor_moves_by_grapheme_cluster() {
        let m = model("a😀e\u{301}b");
        assert_eq!(m.next_grapheme(0), 1);
        assert_eq!(m.next_grapheme(1), 3);
        assert_eq!(m.next_grapheme(3), 5);
        assert_eq!(m.next_grapheme(5), 6);
        assert_eq!(m.next_grapheme(6), 6);
        assert_eq!(m.prev_grapheme(6), 5);
        assert_eq!(m.prev_grapheme(5), 3);
        assert_eq!(m.prev_grapheme(3), 1);
        assert_eq!(m.prev_grapheme(0), 0);
    }

    #[test]
    fn the_cursor_moves_by_word() {
        let m = model("hello, brave  world");
        assert_eq!(m.next_word(0), 5);
        assert_eq!(m.next_word(5), 7);
        assert_eq!(m.next_word(7), 14);
        assert_eq!(m.next_word(14), 19);
        assert_eq!(m.prev_word(19), 14);
        assert_eq!(m.prev_word(14), 7);
        assert_eq!(m.prev_word(13), 7);
        assert_eq!(m.prev_word(7), 5);
        assert_eq!(m.prev_word(5), 0);
        assert_eq!(m.prev_word(0), 0);
        assert_eq!(m.word_end(0), 5);
        assert_eq!(m.word_end(12), 19);
    }

    /// The table Qt 6.11's `TextInput.selectWord()` gives for this text.
    #[test]
    fn punctuation_is_a_word_and_a_line_break_ends_one() {
        // Ctrl+Right and Ctrl+Left on Qt 6.11.
        let m = model("hello, brave-world foo");
        let rights: Vec<_> =
            std::iter::successors(Some(0), |p| (*p < m.len()).then(|| m.next_word(*p))).collect();
        assert_eq!(rights, [0, 5, 7, 12, 13, 19, 22]);
        let lefts: Vec<_> =
            std::iter::successors(Some(22), |p| (*p > 0).then(|| m.prev_word(*p))).collect();
        assert_eq!(lefts, [22, 19, 13, 12, 7, 5, 0]);
        let m = model("hello\nworld");
        assert_eq!((m.next_word(0), m.next_word(5)), (5, 6));
        let m = model("hello brave\nworld");
        assert_eq!((m.prev_word(17), m.prev_word(12)), (12, 11));
    }

    #[test]
    fn select_word_reaches_the_cursor_and_prefers_the_word_that_starts_there() {
        // `selectWord()` on a Qt 6.11 TextInput.
        let m = model("hello     brave world");
        assert_eq!(m.word_at(9, true), 0..9);
        assert_eq!(m.word_at(6, true), 0..6);
        assert_eq!(m.word_at(10, true), 10..15);
        let m = model("hello, brave-world");
        assert_eq!(m.word_at(5, true), 5..6);
        assert_eq!(m.word_at(6, true), 5..6);
        assert_eq!(m.word_at(9, true), 7..12);
    }

    #[test]
    fn select_word_follows_qt_in_a_one_line_field() {
        let m = model("a   b,c.d  ");
        let want = [
            0..1,
            0..1,
            0..2,
            0..3,
            4..5,
            5..6,
            6..7,
            7..8,
            8..9,
            8..9,
            8..10,
            8..11,
        ];
        for (pos, want) in want.into_iter().enumerate() {
            assert_eq!(m.word_at(pos, true), want, "at {pos}");
        }
        let m = model("foo...bar");
        assert_eq!(m.word_at(4, true), 3..6);
        let m = model("don't");
        assert_eq!(m.word_at(3, true), 3..4);
        let m = model("   ");
        assert_eq!(m.word_at(0, true), 0..0);
        assert_eq!(m.word_at(2, true), 0..2);
    }

    /// And `TextEdit.selectWord()`.
    #[test]
    fn select_word_follows_qt_in_a_document() {
        let m = model("a   b,c.d  ");
        let want = [
            0..1,
            0..1,
            0..1,
            0..1,
            4..5,
            5..6,
            6..7,
            7..8,
            8..9,
            8..9,
            8..9,
            11..11,
        ];
        for (pos, want) in want.into_iter().enumerate() {
            assert_eq!(m.word_at(pos, false), want, "at {pos}");
        }
        let m = model("ab\n\ncd");
        assert_eq!(m.word_at(2, false), 0..2);
        assert_eq!(m.word_at(3, false), 3..3);
        assert_eq!(m.word_at(4, false), 4..6);
    }

    #[test]
    fn move_cursor_selection_by_character_and_by_word() {
        let mut m = model("hello brave world");
        m.move_to(0, false);
        m.move_cursor_selection(5, false);
        assert_eq!((m.selection(), m.cursor()), (0..5, 5));
        m.move_cursor_selection(8, true);
        assert_eq!((m.selection(), m.cursor()), (0..11, 11));
        m.move_cursor_selection(2, false);
        assert_eq!((m.selection(), m.cursor()), (0..2, 2));
        m.move_to(9, false);
        m.move_cursor_selection(2, false);
        assert_eq!((m.selection(), m.cursor()), (2..9, 2));
        m.move_to(9, false);
        m.move_cursor_selection(13, true);
        assert_eq!((m.selection(), m.cursor()), (6..17, 17));
    }

    #[test]
    fn overwrite_replaces_what_follows_but_not_a_line_break() {
        let mut m = model("abc\ndef");
        m.move_to(1, false);
        m.overwrite("X");
        assert_eq!((m.text(), m.cursor()), ("aXc\ndef", 2));
        m.overwrite("YZ");
        assert_eq!((m.text(), m.cursor()), ("aXYZ\ndef", 4));
    }

    #[test]
    fn paragraphs_end_at_line_breaks() {
        let m = model("ab\ncde\n");
        assert_eq!((m.paragraph_start(4), m.paragraph_end(4)), (3, 6));
        assert_eq!((m.paragraph_start(2), m.paragraph_end(2)), (0, 2));
        assert_eq!((m.paragraph_start(7), m.paragraph_end(7)), (7, 7));
    }

    #[test]
    fn a_checkpoint_takes_an_edit_back_without_a_trace() {
        let mut m = model("12");
        m.insert(2, "3");
        let before = m.checkpoint();
        m.replace_selection("x");
        m.rollback(before);
        assert_eq!((m.text(), m.cursor()), ("123", 3));
        m.undo();
        assert_eq!(m.text(), "12");
    }

    #[test]
    fn get_text_takes_positions_in_either_order() {
        let m = model("hello brave world");
        assert_eq!(m.slice(0, 5), "hello");
        assert_eq!(m.slice(5, 0), "hello");
        assert_eq!(m.slice(0, 500), "hello brave world");
    }
}
