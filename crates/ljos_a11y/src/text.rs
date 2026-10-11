//! The text of a node that holds text which can be edited, cut into what
//! AccessKit wants: runs of characters below the node, and a selection
//! that names a run and a character in it.

use std::ops::Range;

use accesskit::{Node, NodeId, Role as AkRole, TextPosition, TextSelection};

/// AccessKit counts the characters of a run in bytes: a run holds this
/// many at most.
const RUN_CHARACTERS: usize = 255;

/// The text a node holds and where its cursor is. The owner decides what
/// the text is: a field that hides what is typed gives the characters it
/// shows, never the ones it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextValue {
    pub value: String,
    /// Where the selection starts and where the cursor is, in bytes of
    /// `value`; equal when nothing is selected.
    pub anchor: usize,
    pub cursor: usize,
    /// Whether the text can be selected but not changed.
    pub read_only: bool,
}

/// One run of a [`TextValue`]: a line or a piece of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    /// The bytes of the value the run holds; a line's run ends with its
    /// line break.
    pub range: Range<usize>,
    /// The UTF-8 length of every character.
    pub character_lengths: Vec<u8>,
    /// The characters words start at.
    pub word_starts: Vec<u8>,
}

impl TextRun {
    /// Writes the run onto an AccessKit node.
    pub fn apply(&self, value: &str, node: &mut Node) {
        node.set_role(AkRole::TextRun);
        node.set_value(value.get(self.range.clone()).unwrap_or_default());
        node.set_character_lengths(self.character_lengths.clone());
        node.set_word_starts(self.word_starts.clone());
    }
}

impl TextValue {
    /// The runs, in order. There is always one, so that the cursor of an
    /// empty text and of an empty last line has a run to be in.
    pub fn runs(&self) -> Vec<TextRun> {
        let mut runs = Vec::new();
        let mut run = TextRun {
            range: 0..0,
            character_lengths: Vec::new(),
            word_starts: Vec::new(),
        };
        let mut in_word = false;
        for (at, c) in self.value.char_indices() {
            let word = c.is_alphanumeric();
            if word && !in_word {
                run.word_starts.push(run.character_lengths.len() as u8);
            }
            in_word = word;
            run.character_lengths.push(c.len_utf8() as u8);
            run.range.end = at + c.len_utf8();
            if c == '\n' || run.character_lengths.len() == RUN_CHARACTERS {
                let next = TextRun {
                    range: run.range.end..run.range.end,
                    character_lengths: Vec::new(),
                    word_starts: Vec::new(),
                };
                runs.push(std::mem::replace(&mut run, next));
            }
        }
        // What is left, which is empty after a line break and for an
        // empty text.
        let open = self.value.is_empty() || self.value.ends_with('\n');
        if !run.character_lengths.is_empty() || open || runs.is_empty() {
            runs.push(run);
        }
        runs
    }

    /// The run and the character in it of a byte of the value. A byte
    /// between two runs is the first of the later one.
    pub fn position(runs: &[TextRun], byte: usize) -> (usize, usize) {
        let last = runs.len().saturating_sub(1);
        for (index, run) in runs.iter().enumerate() {
            if byte < run.range.end || index == last {
                let mut at = run.range.start;
                let mut character = 0;
                for len in &run.character_lengths {
                    if at >= byte {
                        break;
                    }
                    at += usize::from(*len);
                    character += 1;
                }
                return (index, character);
            }
        }
        (0, 0)
    }

    /// The byte of the value a character of a run starts at; the end of
    /// the run for a character it does not have.
    pub fn offset(runs: &[TextRun], run: usize, character: usize) -> Option<usize> {
        let run = runs.get(run)?;
        let before: usize = run
            .character_lengths
            .iter()
            .take(character)
            .map(|len| usize::from(*len))
            .sum();
        Some(run.range.start + before)
    }

    /// The selection for a host whose run `index` has the id
    /// `id(index)`.
    pub fn selection(&self, runs: &[TextRun], id: impl Fn(usize) -> NodeId) -> TextSelection {
        let at = |byte: usize| {
            let (run, character_index) = Self::position(runs, byte.min(self.value.len()));
            TextPosition {
                node: id(run),
                character_index,
            }
        };
        TextSelection {
            anchor: at(self.anchor),
            focus: at(self.cursor),
        }
    }

    /// The bytes of the value a selection of an assistive technology
    /// names (anchor, cursor), with `run_of` telling which run a node is.
    pub fn bytes_of(
        runs: &[TextRun],
        selection: &TextSelection,
        run_of: impl Fn(NodeId) -> Option<usize>,
    ) -> Option<(usize, usize)> {
        let at = |p: &TextPosition| Self::offset(runs, run_of(p.node)?, p.character_index);
        Some((at(&selection.anchor)?, at(&selection.focus)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> TextValue {
        TextValue {
            value: value.to_owned(),
            ..TextValue::default()
        }
    }

    #[test]
    fn an_empty_text_has_one_empty_run() {
        let runs = text("").runs();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].range, 0..0);
        assert_eq!(TextValue::position(&runs, 0), (0, 0));
    }

    #[test]
    fn a_line_is_a_run_that_ends_with_its_break() {
        let runs = text("ab\ncd\n").runs();
        let ranges: Vec<_> = runs.iter().map(|r| r.range.clone()).collect();
        assert_eq!(ranges, [0..3, 3..6, 6..6]);
        // After a break is the start of the next line.
        assert_eq!(TextValue::position(&runs, 3), (1, 0));
        assert_eq!(TextValue::position(&runs, 2), (0, 2));
        assert_eq!(TextValue::position(&runs, 6), (2, 0));
        assert_eq!(TextValue::offset(&runs, 1, 2), Some(5));
        assert_eq!(TextValue::offset(&runs, 3, 0), None);
    }

    #[test]
    fn characters_are_counted_not_bytes() {
        let runs = text("né x").runs();
        assert_eq!(runs[0].character_lengths, [1, 2, 1, 1]);
        assert_eq!(runs[0].word_starts, [0, 3]);
        // The byte after the two of `é`.
        assert_eq!(TextValue::position(&runs, 3), (0, 2));
        assert_eq!(TextValue::offset(&runs, 0, 2), Some(3));
        // The end of the text is a place too.
        assert_eq!(TextValue::position(&runs, 5), (0, 4));
    }

    #[test]
    fn a_long_line_is_cut_into_runs() {
        let runs = text(&"a".repeat(300)).runs();
        let ranges: Vec<_> = runs.iter().map(|r| r.range.clone()).collect();
        assert_eq!(ranges, [0..255, 255..300]);
        assert_eq!(TextValue::position(&runs, 255), (1, 0));
        assert_eq!(TextValue::position(&runs, 300), (1, 45));
    }

    #[test]
    fn a_selection_goes_to_runs_and_back() {
        let t = TextValue {
            value: "one\ntwo".into(),
            anchor: 1,
            cursor: 6,
            read_only: false,
        };
        let runs = t.runs();
        let sel = t.selection(&runs, |i| NodeId(100 + i as u64));
        assert_eq!(
            (sel.anchor.node, sel.anchor.character_index),
            (NodeId(100), 1)
        );
        assert_eq!(
            (sel.focus.node, sel.focus.character_index),
            (NodeId(101), 2)
        );
        let back = TextValue::bytes_of(&runs, &sel, |n| Some((n.0 - 100) as usize));
        assert_eq!(back, Some((1, 6)));
    }
}
