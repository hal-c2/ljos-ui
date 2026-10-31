//! Rebasing one writer's [`Change`]s over another's: operational
//! transformation of byte ranges, for one text that two writers changed from
//! the same version.
//!
//! The shape is the one a central order needs (Jupiter, Nichols et al.,
//! 1995): a server has applied `over`, and a change made against the text
//! before it arrives. [`transform`] says where that change lands in the text
//! after `over`, or refuses it when the two touch the same bytes. Nothing is
//! merged inside a hunk: a refusal names the hunk, and the writer whose
//! change it was decides.
//!
//! # The rule
//!
//! Both `change` and `over` are ranges of the same base text. For each hunk
//! of `over`:
//!
//! - wholly before `change` (it ends where `change` starts, or earlier): the
//!   change moves by what the hunk added or took away;
//! - wholly after it (it starts where `change` ends, or later): nothing;
//! - an insertion at the very point where `change` also inserts: the two
//!   are not ordered by the text, so [`Ties`] orders them;
//! - an insertion where `change` replaces a range starts or ends goes before
//!   or after that range, as the text says;
//! - anything else shares bytes with `change`, or is inside what it replaces,
//!   and is an [`Overlap`].
//!
//! Positions only move by whole replacements, so a position on a character
//! boundary of the base text is on one of the new text too.
//!
//! The transform is pure and promises nothing about the result's syntax:
//! the text it gives may not parse, and whoever applies it checks that, as
//! [`apply`](crate::apply) checks its own.

use std::ops::Range;

use crate::Change;

/// Which of two insertions at one point comes first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ties {
    /// The hunk of `over` stays first and the change goes after it: what was
    /// applied first stays first. What a central order uses.
    #[default]
    OverFirst,
    /// The change goes before the hunk of `over`. The other side of
    /// [`OverFirst`](Self::OverFirst): transforming `over` by `change` with
    /// one and `change` by `over` with the other gives one text both ways.
    ChangeFirst,
}

/// A change that touches bytes a hunk it is transformed over touched too.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "the change of bytes {}..{} overlaps the change of bytes {}..{} made before it",
    change.start, change.end, with.start, with.end
)]
pub struct Overlap {
    /// The change's range in the base text.
    pub change: Range<usize>,
    /// The index in `over` of the hunk it overlaps, and that hunk's range in
    /// the base text.
    pub index: usize,
    pub with: Range<usize>,
}

/// Where `change`, made against a base text, lands in the text `over` made
/// of the same base. `over` is sorted and its hunks do not overlap, as
/// [`Applied::changes`](crate::Applied::changes) are; an insertion where a
/// hunk of `over` inserts too goes after it ([`Ties::OverFirst`]).
///
/// Rebasing over several edits applied one after another is a fold: each
/// edit's changes are ranges of the text the one before it made.
///
/// # Errors
/// [`Overlap`], naming the first hunk of `over` the change touches.
pub fn transform(change: &Change, over: &[Change]) -> Result<Change, Overlap> {
    transform_with(change, over, Ties::OverFirst)
}

/// [`transform`], with the order of two insertions at one point chosen.
///
/// # Errors
/// [`Overlap`], naming the first hunk of `over` the change touches.
pub fn transform_with(change: &Change, over: &[Change], ties: Ties) -> Result<Change, Overlap> {
    debug_assert!(
        over.windows(2)
            .all(|pair| pair[0].range.end <= pair[1].range.start),
        "the changes transformed over are sorted and do not overlap"
    );
    let range = &change.range;
    let mut shift = 0isize;
    for (index, hunk) in over.iter().enumerate() {
        let before = match place(range, &hunk.range, ties) {
            Place::Before => true,
            Place::After => false,
            Place::Overlaps => {
                return Err(Overlap {
                    change: range.clone(),
                    index,
                    with: hunk.range.clone(),
                });
            }
        };
        if before {
            shift += hunk.replacement.len() as isize - hunk.range.len() as isize;
        }
    }
    Ok(Change {
        range: range.start.saturating_add_signed(shift)..range.end.saturating_add_signed(shift),
        replacement: change.replacement.clone(),
    })
}

/// [`transform`] for every change of a batch made against one base text
/// (sorted, not overlapping). The answer is sorted and does not overlap
/// either, so it can be [`splice`](crate::splice)d whole.
///
/// # Errors
/// [`Overlap`] for the first change of `changes` that has one.
pub fn transform_all(changes: &[Change], over: &[Change]) -> Result<Vec<Change>, Overlap> {
    transform_all_with(changes, over, Ties::OverFirst)
}

/// [`transform_all`], with the order of two insertions at one point chosen.
///
/// # Errors
/// [`Overlap`] for the first change of `changes` that has one.
pub fn transform_all_with(
    changes: &[Change],
    over: &[Change],
    ties: Ties,
) -> Result<Vec<Change>, Overlap> {
    changes
        .iter()
        .map(|change| transform_with(change, over, ties))
        .collect()
}

/// Where a hunk of the other side is, seen from the change.
enum Place {
    /// It comes first: the change moves by it.
    Before,
    /// It comes after: the change stays.
    After,
    Overlaps,
}

fn place(change: &Range<usize>, hunk: &Range<usize>, ties: Ties) -> Place {
    if change.is_empty() && hunk.is_empty() && change.start == hunk.start {
        return match ties {
            Ties::OverFirst => Place::Before,
            Ties::ChangeFirst => Place::After,
        };
    }
    // Touching ends are no overlap: an insertion at a range's start is
    // before it and one at its end after it, whichever side it is on.
    if hunk.end <= change.start {
        Place::Before
    } else if hunk.start >= change.end {
        Place::After
    } else {
        Place::Overlaps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::splice;
    use proptest::prelude::*;

    fn change(range: Range<usize>, text: &str) -> Change {
        Change {
            range,
            replacement: text.to_owned(),
        }
    }

    #[test]
    fn a_change_after_a_hunk_moves_by_what_it_added() {
        let base = "Item { width: 1; height: 2 }";
        let over = [change(14..15, "100")];
        let mine = change(25..26, "20");
        let moved = transform(&mine, &over).unwrap();
        assert_eq!(moved.range, 27..28);
        let text = splice(&splice(base, &over), &[moved]);
        assert_eq!(text, "Item { width: 100; height: 20 }");
    }

    #[test]
    fn a_change_before_a_hunk_stays() {
        let over = [change(20..22, "")];
        assert_eq!(transform(&change(3..4, "x"), &over).unwrap().range, 3..4);
    }

    #[test]
    fn two_insertions_at_one_point_keep_the_first_applied_first() {
        let base = "ab";
        let over = [change(1..1, "1")];
        let mine = change(1..1, "2");
        let text = splice(&splice(base, &over), &[transform(&mine, &over).unwrap()]);
        assert_eq!(text, "a12b");
        let mine_first = transform_with(&mine, &over, Ties::ChangeFirst).unwrap();
        assert_eq!(splice(&splice(base, &over), &[mine_first]), "a21b");
    }

    #[test]
    fn an_insertion_at_either_end_of_a_replaced_range_does_not_overlap_it() {
        let base = "abcd";
        let over = [change(1..3, "XY")];
        let at_start = transform(&change(1..1, "<"), &over).unwrap();
        assert_eq!(splice(&splice(base, &over), &[at_start]), "a<XYd");
        let at_end = transform(&change(3..3, ">"), &over).unwrap();
        assert_eq!(splice(&splice(base, &over), &[at_end]), "aXY>d");
    }

    #[test]
    fn a_hunk_inserted_at_either_end_of_the_changed_range_does_not_overlap_it() {
        let base = "abcd";
        let mine = change(1..3, "XY");
        let at_start = [change(1..1, "<")];
        let moved = transform(&mine, &at_start).unwrap();
        assert_eq!(splice(&splice(base, &at_start), &[moved]), "a<XYd");
        let at_end = [change(3..3, ">")];
        let stays = transform(&mine, &at_end).unwrap();
        assert_eq!(splice(&splice(base, &at_end), &[stays]), "aXY>d");
    }

    #[test]
    fn ranges_that_share_bytes_are_refused_naming_the_hunk() {
        let over = [change(0..1, "z"), change(4..8, "")];
        let err = transform(&change(6..10, "x"), &over).unwrap_err();
        assert_eq!(
            err,
            Overlap {
                change: 6..10,
                index: 1,
                with: 4..8
            }
        );
        assert_eq!(
            err.to_string(),
            "the change of bytes 6..10 overlaps the change of bytes 4..8 made before it"
        );
    }

    #[test]
    fn an_insertion_inside_a_replaced_range_is_refused_from_either_side() {
        assert!(transform(&change(2..2, "x"), &[change(1..3, "")]).is_err());
        assert!(transform(&change(1..3, ""), &[change(2..2, "x")]).is_err());
    }

    #[test]
    fn the_same_change_twice_is_refused() {
        assert!(transform(&change(1..2, "x"), &[change(1..2, "x")]).is_err());
    }

    /// A text and a batch of changes of it, sorted and not overlapping.
    fn text_and_batch() -> impl Strategy<Value = (String, Vec<Change>)> {
        "[a-cé\n]{0,12}".prop_flat_map(|text| {
            let bounds: Vec<usize> = text
                .char_indices()
                .map(|(i, _)| i)
                .chain([text.len()])
                .collect();
            let n = bounds.len();
            let batch = proptest::collection::vec((0..n, 0..n, "[xyz]{0,3}"), 0..4);
            (Just(text), batch).prop_map(move |(text, raw)| {
                let mut changes: Vec<Change> = raw
                    .into_iter()
                    .map(|(a, b, r)| {
                        let (a, b) = (a.min(b), a.max(b));
                        change(bounds[a]..bounds[b], &r)
                    })
                    .collect();
                changes.sort_by_key(|c| (c.range.start, c.range.end));
                // Keep a batch apply could have made: none overlapping, and
                // no two insertions at one point.
                let mut kept: Vec<Change> = Vec::new();
                for c in changes {
                    if kept.last().is_none_or(|last| {
                        last.range.end < c.range.start
                            || (last.range.end == c.range.start
                                && !(last.range.is_empty() && c.range.is_empty()))
                    }) {
                        kept.push(c);
                    }
                }
                (text, kept)
            })
        })
    }

    fn two_batches() -> impl Strategy<Value = (String, Vec<Change>, Vec<Change>)> {
        text_and_batch().prop_flat_map(|(text, a)| {
            let len = text.len();
            let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([len]).collect();
            let n = bounds.len();
            proptest::collection::vec((0..n, 0..n, "[pq]{0,3}"), 0..4).prop_map(move |raw| {
                let mut b: Vec<Change> = raw
                    .into_iter()
                    .map(|(x, y, r)| {
                        let (x, y) = (x.min(y), x.max(y));
                        change(bounds[x]..bounds[y], &r)
                    })
                    .collect();
                b.sort_by_key(|c| (c.range.start, c.range.end));
                let mut kept: Vec<Change> = Vec::new();
                for c in b {
                    if kept.last().is_none_or(|last| {
                        last.range.end < c.range.start
                            || (last.range.end == c.range.start
                                && !(last.range.is_empty() && c.range.is_empty()))
                    }) {
                        kept.push(c);
                    }
                }
                (text.clone(), a.clone(), kept)
            })
        })
    }

    proptest! {
        /// TP1: whichever batch is applied first, the other rebased over it
        /// gives the same text, when the tie goes the same way both times.
        #[test]
        fn both_orders_converge((base, a, b) in two_batches()) {
            let b_over_a = transform_all_with(&b, &a, Ties::OverFirst);
            let a_over_b = transform_all_with(&a, &b, Ties::ChangeFirst);
            prop_assert_eq!(b_over_a.is_ok(), a_over_b.is_ok(), "an overlap is seen from both sides");
            if let (Ok(b2), Ok(a2)) = (b_over_a, a_over_b) {
                let one = splice(&splice(&base, &a), &b2);
                let other = splice(&splice(&base, &b), &a2);
                prop_assert_eq!(one, other);
            }
        }

        /// A rebased batch is still sorted and does not overlap, so it can
        /// be spliced whole, and it lands on character boundaries.
        #[test]
        fn a_rebased_batch_is_a_batch((base, a, b) in two_batches()) {
            if let Ok(b2) = transform_all(&b, &a) {
                let after = splice(&base, &a);
                for pair in b2.windows(2) {
                    prop_assert!(pair[0].range.end <= pair[1].range.start);
                }
                for c in &b2 {
                    prop_assert!(c.range.end <= after.len());
                    prop_assert!(after.is_char_boundary(c.range.start));
                    prop_assert!(after.is_char_boundary(c.range.end));
                }
            }
        }

        /// What the other side did not touch is what each rebased change
        /// replaces: its bytes in the new text are its bytes in the base.
        #[test]
        fn a_rebased_change_replaces_the_same_bytes((base, a, b) in two_batches()) {
            let after = splice(&base, &a);
            for c in &b {
                if let Ok(c2) = transform(c, &a) {
                    prop_assert_eq!(&after[c2.range.clone()], &base[c.range.clone()]);
                }
            }
        }

        /// Rebasing over nothing changes nothing.
        #[test]
        fn rebasing_over_nothing_is_the_identity((_base, a) in text_and_batch()) {
            prop_assert_eq!(transform_all(&a, &[]).unwrap(), a);
        }
    }
}
