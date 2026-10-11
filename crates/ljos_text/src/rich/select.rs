//! One selection over several documents (ARCHITECTURE §7.5).
//!
//! A [`SelectionScope`] strings its members together in reading order into
//! one space of offsets, a line break between two members, so a selection
//! is a range of that space wherever it starts and ends. A member is known
//! by an id and a length only: its text is asked for when it is needed, so
//! a member a view has not made (or has let go of) still takes part as long
//! as whoever owns the scope can say what it holds.

use std::ops::Range;

/// Offsets count bytes of each member's document text
/// ([`super::RichDoc::text`]), plus one for the break after every member
/// but the last.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionScope {
    /// Id, start in the scope's offsets, and length.
    members: Vec<(u64, usize, usize)>,
    /// Where the selection was started and where it reaches.
    anchor: usize,
    focus: usize,
}

impl SelectionScope {
    pub fn new() -> Self {
        Self::default()
    }

    /// Name the members, in reading order, and how long each one's text is.
    /// The selection is kept where it still makes sense: its ends are held
    /// inside the new length.
    pub fn set_members(&mut self, members: impl IntoIterator<Item = (u64, usize)>) {
        self.members.clear();
        let mut at = 0_usize;
        for (id, len) in members {
            self.members.push((id, at, len));
            at = at.saturating_add(len).saturating_add(1);
        }
        let len = self.len();
        self.anchor = self.anchor.min(len);
        self.focus = self.focus.min(len);
    }

    /// The ids of the members, in order.
    pub fn members(&self) -> impl Iterator<Item = u64> + '_ {
        self.members.iter().map(|m| m.0)
    }

    /// Length of the whole offset space.
    pub fn len(&self) -> usize {
        self.members
            .last()
            .map_or(0, |(_, start, len)| start.saturating_add(*len))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The scope's offset of `local` in member `id`.
    pub fn offset_of(&self, id: u64, local: usize) -> Option<usize> {
        self.members
            .iter()
            .find(|m| m.0 == id)
            .map(|(_, start, len)| start + local.min(*len))
    }

    /// The member an offset is in and where in it. The break after a member
    /// counts as that member's end.
    pub fn locate(&self, offset: usize) -> Option<(u64, usize)> {
        let i = self
            .members
            .partition_point(|(_, start, len)| start + len < offset);
        let (id, start, len) = self.members.get(i).or(self.members.last())?;
        Some((*id, offset.saturating_sub(*start).min(*len)))
    }

    /// Select from `anchor` to `focus`, in either order.
    pub fn select(&mut self, anchor: usize, focus: usize) {
        let len = self.len();
        self.anchor = anchor.min(len);
        self.focus = focus.min(len);
    }

    /// Move the end of the selection that moves.
    pub fn extend_to(&mut self, focus: usize) {
        self.focus = focus.min(self.len());
    }

    pub fn clear(&mut self) {
        self.focus = self.anchor;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.focus = self.len();
    }

    pub fn anchor(&self) -> usize {
        self.anchor
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    /// What is selected, start first.
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.focus
    }

    /// The part of member `id` that is selected, in its own offsets. None
    /// when nothing of it is.
    pub fn range_in(&self, id: u64) -> Option<Range<usize>> {
        let (_, start, len) = self.members.iter().find(|m| m.0 == id)?;
        let range = self.range();
        if range.is_empty() || range.end <= *start || range.start > start + len {
            return None;
        }
        let from = range.start.saturating_sub(*start);
        let to = (range.end - start).min(*len);
        (from < to).then_some(from..to)
    }

    /// The selected text. `text` gives the text of a range of a member; it
    /// is asked only for members the selection touches.
    pub fn selected_text(&self, mut text: impl FnMut(u64, Range<usize>) -> String) -> String {
        let range = self.range();
        let mut out = String::new();
        if range.is_empty() {
            return out;
        }
        for (id, start, len) in &self.members {
            let end = start + len;
            if range.end <= *start || range.start > end {
                continue;
            }
            let from = range.start.saturating_sub(*start);
            let to = (range.end - start).min(*len);
            if from < to {
                out.push_str(&text(*id, from..to));
            }
            // The break between this member and the next is selected too.
            if range.end > end && range.start <= end {
                out.push('\n');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> SelectionScope {
        let mut s = SelectionScope::new();
        // "abc", "", "defgh"
        s.set_members([(10, 3), (11, 0), (12, 5)]);
        s
    }

    fn text(id: u64, range: Range<usize>) -> String {
        let all = match id {
            10 => "abc",
            11 => "",
            _ => "defgh",
        };
        all[range].to_owned()
    }

    #[test]
    fn members_share_one_offset_space() {
        let s = scope();
        assert_eq!(s.len(), 10);
        assert_eq!(s.offset_of(10, 2), Some(2));
        assert_eq!(s.offset_of(11, 0), Some(4));
        assert_eq!(s.offset_of(12, 1), Some(6));
        assert_eq!(s.offset_of(12, 99), Some(10));
        assert_eq!(s.offset_of(99, 0), None);
        assert_eq!(s.locate(0), Some((10, 0)));
        assert_eq!(s.locate(3), Some((10, 3)));
        assert_eq!(s.locate(4), Some((11, 0)));
        assert_eq!(s.locate(5), Some((12, 0)));
        assert_eq!(s.locate(10), Some((12, 5)));
        assert_eq!(s.locate(900), Some((12, 5)));
        assert_eq!(SelectionScope::new().locate(0), None);
        for offset in 0..=s.len() {
            let (id, local) = s.locate(offset).expect("a member");
            assert_eq!(s.offset_of(id, local), Some(offset));
        }
    }

    #[test]
    fn a_selection_runs_across_members() {
        let mut s = scope();
        s.select(7, 1);
        assert_eq!(s.range(), 1..7);
        assert_eq!(s.range_in(10), Some(1..3));
        assert_eq!(s.range_in(11), None);
        assert_eq!(s.range_in(12), Some(0..2));
        assert_eq!(s.selected_text(text), "bc\n\nde");
        s.select_all();
        assert_eq!(s.selected_text(text), "abc\n\ndefgh");
        s.select(1, 2);
        assert_eq!(s.selected_text(text), "b");
        assert_eq!(s.range_in(12), None);
        // Just the break between two members.
        s.select(3, 4);
        assert_eq!(s.selected_text(text), "\n");
        s.clear();
        assert!(!s.has_selection());
        assert_eq!(s.selected_text(text), "");
    }

    #[test]
    fn only_touched_members_are_asked_for_text() {
        let mut s = scope();
        s.select(5, 8);
        let mut asked = Vec::new();
        let out = s.selected_text(|id, range| {
            asked.push(id);
            text(id, range)
        });
        assert_eq!(out, "def");
        assert_eq!(asked, [12]);
    }

    #[test]
    fn a_selection_is_held_inside_the_members() {
        let mut s = scope();
        s.select(2, 900);
        assert_eq!(s.range(), 2..10);
        s.set_members([(10, 3)]);
        assert_eq!(s.range(), 2..3);
        s.extend_to(0);
        assert_eq!(s.range(), 0..2);
        assert_eq!((s.anchor(), s.focus()), (2, 0));
        s.set_members([]);
        assert!(s.is_empty());
        assert_eq!(s.range(), 0..0);
        assert_eq!(s.members().count(), 0);
    }
}
