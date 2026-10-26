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

use ljos_design::limits::keyboard::ROW_OVERLAP;
use ljos_render::kurbo::Rect;

use crate::cx::{Cx, place_of, scene_rect, shown_items, words};
use crate::focus_walk::rounds;

fn row_overlap(a: Rect, b: Rect) -> bool {
    let shared = a.y1.min(b.y1) - a.y0.max(b.y0);
    shared >= f64::from(ROW_OVERLAP) * a.height().min(b.height()) && shared > 0.0
}

fn column_overlap(a: Rect, b: Rect) -> bool {
    a.x1.min(b.x1) - a.x0.max(b.x0) > 0.0
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let drawn: std::collections::HashSet<_> = shown_items(cx).into_iter().collect();
    let own = |s| place_of(e, s).is_some_and(|p| !ljos_controls::is_style_url(&p));
    let mut found = Vec::new();
    for (_, round) in rounds(cx) {
        let placed: Vec<_> = round
            .into_iter()
            .filter(|s| drawn.contains(s))
            .filter_map(|s| scene_rect(e, s).map(|r| (s, r)))
            .filter(|(_, r)| r.width() >= 1.0 && r.height() >= 1.0)
            .collect();
        for pair in placed.windows(2) {
            let ((from, a), (to, b)) = (pair[0], pair[1]);
            // Nested stops (a field inside a spin box) are one place.
            if a.contains_rect(b) || b.contains_rect(a) {
                continue;
            }
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
