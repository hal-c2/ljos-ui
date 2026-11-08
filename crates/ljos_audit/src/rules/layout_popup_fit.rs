//! `layout-popup-fit`: a popup far larger than what it holds, or too
//! small for it.
//!
//! Each open popup's size against its implicit size, which is what its
//! content, header, footer and padding ask for: filled to less than
//! `limits::POPUP_MIN_FILL` of its height it is too large (a width is
//! often one menus and dialogs share, so it is not held to its content);
//! asked for more than it has on a side it is too small, unless the
//! scrollers shown on the way down from its content can give back what is
//! missing, and of no size at all it hides what it holds. A popup reaching
//! past the window is too large for it. Tool tips size themselves and are
//! left out, and a drawer is not held to its fill: it runs along an edge
//! of the window by design, whatever it holds.

use ljos_controls::{Drawer, ToolTip, item_popup};
use ljos_design::limits::POPUP_MIN_FILL;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{
    Column, ColumnLayout, Layout, Row, RowLayout, StackLayout, child_items, child_sizes,
};

use crate::cx::{
    Cx, is_a, is_scroller, number, overhang, overhang_words, scene_rect, shown_items, slack, trim,
};

/// Whether the scrollers in the content can give back what the popup is
/// missing, `over` px across and `down` px high: on each side that falls
/// short, what the content asks for less what it can scroll through is no
/// more than the popup has.
fn content_scrolls(engine: &Engine, content: ObjectId, over: f64, down: f64) -> bool {
    let takes = |axis, missing: f64| {
        let mut budget = crate::cx::DEPTH;
        let asks = Asks {
            size: number(engine, content, IMPLICIT[axis]).max(0.0),
            least: 0.0,
        };
        missing <= 0.0 || gives(engine, content, axis, asks, &mut budget) >= missing
    };
    takes(H, over) && takes(V, down)
}

const H: usize = 0;
const V: usize = 1;
const IMPLICIT: [&str; 2] = ["implicitWidth", "implicitHeight"];

/// What an item asks of what holds it along an axis, and the least it
/// can be made.
#[derive(Clone, Copy)]
struct Asks {
    size: f64,
    least: f64,
}

/// What `c` asks of `parent` along `axis`: of a layout, the size the
/// layout gives it and the least it can make it ([`child_sizes`]), the
/// same for a child that does not fill; of a layout that leaves the size
/// to the child, its implicit size, kept; of what else holds it, its
/// implicit size, which a scroller can give back.
fn asks_of(engine: &Engine, parent: ObjectId, c: ObjectId, axis: usize) -> Asks {
    let implicit = number(engine, c, IMPLICIT[axis]).max(0.0);
    match child_sizes(engine, parent, c, axis == V) {
        Some(s) => Asks {
            size: s.preferred.max(0.0),
            least: s.minimum.clamp(0.0, s.preferred.max(0.0)),
        },
        None if is_a::<Layout>(engine, parent) => Asks {
            size: implicit,
            least: implicit,
        },
        None => Asks {
            size: implicit,
            least: 0.0,
        },
    }
}

/// How much of what `o` asks for along `axis` it can give back by
/// scrolling. A page may sit as deep as the app's layouts put it (a row,
/// a column, a loader, a stack of pages), so this follows the path down
/// from the content. A scroller can give back all it asks for, and what
/// is inside one is bounded by it. One after another along the axis,
/// children give back what each of them can; side by side across it, or
/// on top of each other, the one that gives back least is the floor, as
/// a row is as high as its tallest fixed child. Hidden items are left
/// out as layouts leave them out, but every page of a stack sizes it: the
/// stack lays out the page shown alone, so a hidden one holds it no
/// larger than the least that page can be made. A container placing its
/// children some other way is taken to give back what its largest child
/// asks for less what the one that keeps most keeps: what it asks for
/// past its children (spacing, margins, tracks side by side) it keeps,
/// so with no scroller in it, it gives back nothing. What each child asks
/// is what `o` sizes it by ([`asks_of`]); a scroller gives back what it
/// asks for past the least it can be made, and no child gives back more
/// than that.
fn gives(engine: &Engine, o: ObjectId, axis: usize, own: Asks, budget: &mut usize) -> f64 {
    let Some(left) = budget.checked_sub(1) else {
        return 0.0;
    };
    *budget = left;
    let asks = own.size;
    if is_scroller(engine, o) {
        return (asks - own.least).max(0.0);
    }
    let stack = is_a::<StackLayout>(engine, o);
    let mut along = 0.0;
    let mut floor: f64 = 0.0;
    let mut widest: f64 = 0.0;
    let mut any = false;
    for c in child_items(engine, o) {
        let child = asks_of(engine, o, c, axis);
        let given = if shown(engine, c) {
            // Nothing inside a child scrolls it smaller than its parent
            // lets it be: one kept at its size gives back nothing.
            gives(engine, c, axis, child, budget).min((child.size - child.least).max(0.0))
        } else if stack {
            (child.size - child.least).max(0.0)
        } else {
            continue;
        };
        any = true;
        along += given;
        let child_asks = child.size;
        widest = widest.max(child_asks);
        floor = floor.max(child_asks - given);
    }
    if !any {
        return 0.0;
    }
    let one_after_another = match axis {
        H => is_a::<RowLayout>(engine, o) || is_a::<Row>(engine, o),
        _ => is_a::<ColumnLayout>(engine, o) || is_a::<Column>(engine, o),
    };
    if one_after_another {
        along.min(asks)
    } else {
        (widest - floor).max(0.0)
    }
}

fn shown(engine: &Engine, o: ObjectId) -> bool {
    engine
        .untrack(|| engine.read_by_name(o, "visible"))
        .to_boolean()
}

fn name_of(engine: &Engine, popup: ObjectId) -> String {
    let ty = engine
        .meta_of(popup)
        .map(|m| m.name.to_string())
        .unwrap_or_else(|| "Popup".into());
    let name = engine
        .untrack(|| engine.read_by_name(popup, "objectName"))
        .to_js_string();
    if name.is_empty() {
        format!("the {ty}")
    } else {
        format!("the {ty} {:?}", name.as_str())
    }
}

/// A descendant of `content` that is shown and has a size: something a
/// popup of no size hides.
fn something_shown(engine: &Engine, content: ObjectId) -> Option<ObjectId> {
    let mut stack = child_items(engine, content);
    let mut seen = 0;
    while let Some(o) = stack.pop() {
        seen += 1;
        if seen > crate::cx::DEPTH {
            return None;
        }
        if !shown(engine, o) {
            continue;
        }
        if number(engine, o, "width") > 0.0 && number(engine, o, "height") > 0.0 {
            return Some(o);
        }
        stack.extend(child_items(engine, o));
    }
    None
}

pub(crate) fn check(cx: &mut Cx) {
    let engine = cx.engine;
    let window = cx.scene.viewport();
    for item in shown_items(cx) {
        let Some(popup) = item_popup(engine, item) else {
            continue;
        };
        if is_a::<ToolTip>(engine, popup) {
            continue;
        }
        let what = name_of(engine, popup);
        let (aw, ah) = (
            number(engine, popup, "width"),
            number(engine, popup, "height"),
        );
        let (cw, ch) = (
            number(engine, popup, "implicitWidth"),
            number(engine, popup, "implicitHeight"),
        );
        let has = format!("{}x{}", trim(aw), trim(ah));
        let asks = format!("{}x{}", trim(cw), trim(ch));
        if let Some(rect) = scene_rect(engine, item)
            && let Some(past) = overhang_words(overhang(rect, window))
        {
            let message = format!(
                "{what} is {}x{} and reaches past {past} of the {}x{} window",
                trim(rect.width()),
                trim(rect.height()),
                window.width(),
                window.height()
            );
            let f = cx.report("layout-popup-fit", Some(popup), message);
            f.measured = Some(format!("past {past}"));
            continue;
        }
        let content = engine
            .untrack(|| engine.read_by_name(popup, "contentItem"))
            .as_object();
        if (aw <= slack() || ah <= slack())
            && let Some(shows) = content.and_then(|c| something_shown(engine, c))
        {
            let message = format!(
                "{what} is {has} px and shows nothing of what it holds, such as {}",
                crate::cx::words(cx, shows)
            );
            let f = cx.report("layout-popup-fit", Some(popup), message);
            f.measured = Some(format!("{has}, collapsed"));
            continue;
        }
        if !(aw > 0.0 && ah > 0.0 && cw.is_finite() && ch.is_finite()) {
            continue;
        }
        let (over, down) = (cw - aw - slack(), ch - ah - slack());
        if (over > 0.0 || down > 0.0)
            && !content.is_some_and(|c| content_scrolls(engine, c, over, down))
        {
            let message = format!(
                "{what} is {has} px where what it holds asks for {asks}, and nothing in it \
                 scrolls through the rest"
            );
            let f = cx.report("layout-popup-fit", Some(popup), message);
            f.measured = Some(format!("{has} < {asks}"));
            continue;
        }
        // Content that sizes to nothing, the popup asking for its padding
        // alone, is laid out by hand; its fill cannot be told.
        let content_height =
            ch - number(engine, popup, "topPadding") - number(engine, popup, "bottomPadding");
        if content_height <= 0.0 || ch <= 0.0 || is_a::<Drawer>(engine, popup) {
            continue;
        }
        let fill = ch.min(ah) / ah;
        if fill < f64::from(POPUP_MIN_FILL) {
            let message = format!(
                "{what} is {} px high and what it holds takes {}, {:.0} % of it",
                trim(ah),
                trim(ch),
                fill * 100.0
            );
            let f = cx.report("layout-popup-fit", Some(popup), message);
            f.measured = Some(format!(
                "fills {:.0} % < {:.0} %",
                fill * 100.0,
                POPUP_MIN_FILL * 100.0
            ));
        }
    }
}
