//! The rows of a menu and of a combo box's popup, when there are more of
//! them than fit.
//!
//! TODO(listview): Qt shows these rows in a `ListView`. Until there is
//! one, a `Flickable` given as the content item stands in for it: the
//! rows are children of what it scrolls, that is as high as the rows
//! together, and the row the keyboard is on is kept in view. Any other
//! content item holds the rows itself and does not scroll.
//!
//! Where the rows end up when a key moves through them or the popup opens
//! was read off a running Qt 6.11.2; `features/controls/menu.feature` and
//! `combobox.feature` pin it. Qt animates the move, which this does not.

use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::{Flickable, Item, flickable};

use crate::util::{check, is_unset, num};

/// The item the rows are children of: what `content` scrolls, or
/// `content` itself where it does not scroll.
pub(crate) fn home(engine: &Engine, content: ObjectId) -> ObjectId {
    flickable::content_item(engine, content).unwrap_or(content)
}

/// Whether `content` scrolls its rows.
pub(crate) fn scrolls(engine: &Engine, content: ObjectId) -> bool {
    flickable::is_flickable(engine, content)
}

/// Says how high the rows are together, where the author of the content
/// item did not: a derived value, so one given later wins.
pub(crate) fn set_extent(engine: &Engine, content: ObjectId, height: f64) {
    if !scrolls(engine, content) {
        return;
    }
    let p = Flickable::CONTENT_HEIGHT.index;
    engine.untrack(|| {
        if is_unset(engine, content, p) && num(engine, content, p) != height {
            check(
                engine,
                "contentHeight",
                engine.write_derived(content, p, Value::Real(height)),
            );
        }
    });
}

/// Where a row is in what scrolls: its top and its bottom.
fn span(engine: &Engine, row: ObjectId) -> (f64, f64) {
    let top = num(engine, row, Item::Y.index);
    (top, top + num(engine, row, Item::HEIGHT.index))
}

fn scroll_to(engine: &Engine, content: ObjectId, wanted: f64) {
    let axis = flickable::axis_state(engine, content, false);
    let to = wanted.min(axis.max).max(axis.min);
    if to != axis.position {
        flickable::scroll(engine, content, false, to);
    }
}

/// Moves the rows as little as shows all of `row`.
pub(crate) fn keep_in_view(engine: &Engine, content: ObjectId, row: ObjectId) {
    if !scrolls(engine, content) {
        return;
    }
    engine.untrack(|| {
        let (top, bottom) = span(engine, row);
        let view = num(engine, content, Item::HEIGHT.index);
        let at = num(engine, content, Flickable::CONTENT_Y.index);
        let to = within(at, view, top, bottom);
        scroll_to(engine, content, to);
    });
}

/// Puts `row` at the top, as far as the rows go.
pub(crate) fn start_at(engine: &Engine, content: ObjectId, row: ObjectId) {
    if !scrolls(engine, content) {
        return;
    }
    engine.untrack(|| {
        let (top, _) = span(engine, row);
        scroll_to(engine, content, top);
    });
}

/// Back to the first row.
pub(crate) fn start(engine: &Engine, content: ObjectId) {
    if scrolls(engine, content) {
        engine.untrack(|| scroll_to(engine, content, 0.0));
    }
}

/// Where a view `view` high that is at `at` goes to show `top..bottom`.
/// A row higher than the view shows its start.
fn within(at: f64, view: f64, top: f64, bottom: f64) -> f64 {
    if top < at || bottom - top > view {
        top
    } else if bottom > at + view {
        bottom - view
    } else {
        at
    }
}

#[cfg(test)]
mod tests {
    use super::within;

    #[test]
    fn a_row_in_view_moves_nothing() {
        assert_eq!(within(40.0, 400.0, 40.0, 80.0), 40.0);
        assert_eq!(within(40.0, 400.0, 400.0, 440.0), 40.0);
    }

    #[test]
    fn a_row_below_comes_to_the_bottom() {
        assert_eq!(within(0.0, 388.0, 390.0, 420.0), 32.0);
        assert_eq!(within(0.0, 400.0, 760.0, 800.0), 400.0);
    }

    #[test]
    fn a_row_above_comes_to_the_top() {
        assert_eq!(within(400.0, 400.0, 360.0, 400.0), 360.0);
    }

    #[test]
    fn a_row_higher_than_the_view_shows_its_start() {
        assert_eq!(within(0.0, 100.0, 50.0, 250.0), 50.0);
    }
}
