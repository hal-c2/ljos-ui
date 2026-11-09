//! `focus-order`: Tab goes against the reading order.
//!
//! Only the jumps no layout explains are reported: from a stop to one on
//! the same row that is behind it in the reading direction (left in a
//! window read left to right), and from a stop to one straight above it
//! in the same column. Going up to the top of the next column, or from
//! the end of a row to the start of the next, is how a page is read; the
//! round's return to its first stop is how Tab goes round.
//!
//! A jump between two stops a style made (a dialog's standard buttons,
//! laid out in the platform's order) is the style's to fix, and left out.
//!
//! Rows are what overlap vertically by `limits::keyboard::ROW_OVERLAP` of
//! the shorter; a column is what overlaps horizontally at all.
//!
//! A stop inside a scroller the other stop is not in is read inside the
//! scroller's viewport: its laid-out place may be far below the fold, but
//! Tab scrolls it in at the viewport's foot, and one above the fold at its
//! head. A stop already in view is read where it is drawn, so a stop drawn
//! over the viewport is still compared with what lies under it.

use ljos_design::limits::keyboard::ROW_OVERLAP;
use ljos_render::kurbo::Rect;

use ljos_engine::{Engine, ObjectId};

use crate::cx::{Cx, ancestors, declared_by_app, is_scroller, scene_rect, shown_items, words};
use crate::focus_walk::rounds;

fn row_overlap(a: Rect, b: Rect) -> bool {
    let shared = a.y1.min(b.y1) - a.y0.max(b.y0);
    shared >= f64::from(ROW_OVERLAP) * a.height().min(b.height()) && shared > 0.0
}

fn column_overlap(a: Rect, b: Rect) -> bool {
    a.x1.min(b.x1) - a.x0.max(b.x0) > 0.0
}

/// The scrollers `item` lies in, outermost first.
fn scrollers(engine: &Engine, item: ObjectId) -> Vec<ObjectId> {
    let mut out: Vec<_> = ancestors(engine, item)
        .filter(|a| is_scroller(engine, *a))
        .collect();
    out.reverse();
    out
}

/// Where the stop is read beside another: its own place when both lie in
/// the same scrollers, otherwise its place brought into the viewport of the
/// outermost scroller it lies in and the other does not.
fn read_at(engine: &Engine, at: Rect, own: &[ObjectId], other: &[ObjectId]) -> Rect {
    let shared = own.iter().zip(other).take_while(|(a, b)| a == b).count();
    own.get(shared)
        .and_then(|s| scene_rect(engine, *s))
        .map_or(at, |view| into_view(at, view))
}

/// `at` moved the least that brings it inside `view`, as scrolling to it
/// would; a stop larger than the view on an axis is read as the view there.
fn into_view(at: Rect, view: Rect) -> Rect {
    fn axis(lo: f64, hi: f64, v_lo: f64, v_hi: f64) -> (f64, f64) {
        if hi - lo >= v_hi - v_lo {
            return (v_lo, v_hi);
        }
        let by = if lo < v_lo {
            v_lo - lo
        } else if hi > v_hi {
            v_hi - hi
        } else {
            0.0
        };
        (lo + by, hi + by)
    }
    let (x0, x1) = axis(at.x0, at.x1, view.x0, view.x1);
    let (y0, y1) = axis(at.y0, at.y1, view.y0, view.y1);
    Rect::new(x0, y0, x1, y1)
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let drawn: std::collections::HashSet<_> = shown_items(cx).into_iter().collect();
    let own = |s| declared_by_app(e, s);
    let mut found = Vec::new();
    for (_, round) in rounds(cx) {
        let placed: Vec<_> = round
            .into_iter()
            .filter(|s| drawn.contains(s))
            .filter_map(|s| scene_rect(e, s).map(|r| (s, r, scrollers(e, s))))
            .filter(|(_, r, _)| r.width() >= 1.0 && r.height() >= 1.0)
            .collect();
        for pair in placed.windows(2) {
            let ((from, a, sa), (to, b, sb)) = (&pair[0], &pair[1]);
            let (from, to) = (*from, *to);
            // Nested stops (a field inside a spin box) are one place. Asked
            // of where they are laid out: read inside a viewport, a stop
            // could land inside one drawn over that viewport without
            // being part of it.
            if a.contains_rect(*b) || b.contains_rect(*a) {
                continue;
            }
            let (a, b) = (read_at(e, *a, sa, sb), read_at(e, *b, sb, sa));
            if !own(from) && !own(to) {
                continue;
            }
            let rtl = ljos_quick::is_mirrored(e, to);
            let back_on_row = row_overlap(a, b) && if rtl { b.x0 >= a.x1 } else { b.x1 <= a.x0 };
            let up_column = column_overlap(a, b) && b.y1 <= a.y0;
            if back_on_row {
                found.push((from, to, "back along the row"));
            } else if up_column {
                found.push((from, to, "back up the column"));
            }
        }
    }
    for (from, to, how) in found {
        let message = format!(
            "Tab goes from {} {how} to {}, against the order the window is read in; order \
             the items in the file as they are read, or set `KeyNavigation.tab`",
            words(cx, from),
            words(cx, to),
        );
        cx.report("focus-order", Some(to), message);
    }
}
