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
use ljos_quick::{Column, Flow, Grid, Layout, Row, child_items, inset_in_parent, least_size_with};

use crate::cx::{
    Cx, is_a, is_scroller, number, overhang, overhang_words, scene_rect, shown_items, slack, trim,
};

/// Whether the scrollers in the content can give back what the popup is
/// missing, `over` px across and `down` px high: on each side that falls
/// short, what the popup's content asks for (`contentWidth`,
/// `contentHeight`) less what it can scroll through is no more than the
/// popup has.
fn content_scrolls(
    engine: &Engine,
    popup: ObjectId,
    content: ObjectId,
    over: f64,
    down: f64,
) -> bool {
    let takes = |axis, missing: f64| {
        let mut budget = crate::cx::DEPTH;
        let asked = number(engine, popup, CONTENT[axis]);
        let size = if asked.is_finite() {
            asked
        } else {
            number(engine, content, IMPLICIT[axis])
        };
        let asks = Asks {
            size: size.max(0.0),
            least: 0.0,
        };
        missing <= 0.0 || gives(engine, content, axis, asks, &mut budget) >= missing
    };
    takes(H, over) && takes(V, down)
}

const H: usize = 0;
const V: usize = 1;
const IMPLICIT: [&str; 2] = ["implicitWidth", "implicitHeight"];
const CONTENT: [&str; 2] = ["contentWidth", "contentHeight"];

/// What an item asks of what holds it along an axis, and the least it
/// can be made.
#[derive(Clone, Copy)]
struct Asks {
    size: f64,
    least: f64,
}

/// How much of what `o` asks for along `axis` it can give back by
/// scrolling. A page may sit as deep as the app's layouts put it (a row,
/// a column, a loader, a stack of pages), so this follows the path down
/// from the content. A scroller gives back what it asks for past the
/// least it can be made, and nothing inside a child scrolls it smaller
/// than what holds it lets it be.
///
/// A layout is asked by its own rules ([`least_size_with`]) how small it
/// can be once each child it sizes is as small as what scrolls in that
/// child lets it be: spacing, margins, cells made alike, spans and a
/// stack's pages are the layout's, not copied here. A hidden page of a
/// stack holds it no smaller than the least that page can be made. A
/// positioner keeps each child at its own size, so it gives back nothing.
/// What else holds its children places them by hand: a child its anchors
/// size from it asks for its size less the margins ([`inset_in_parent`]),
/// another its implicit size; on top of each other, the one that gives
/// back least is the floor.
fn gives(engine: &Engine, o: ObjectId, axis: usize, own: Asks, budget: &mut usize) -> f64 {
    let Some(left) = budget.checked_sub(1) else {
        return 0.0;
    };
    *budget = left;
    if is_scroller(engine, o) {
        return (own.size - own.least).max(0.0);
    }
    if is_a::<Layout>(engine, o) {
        let least = least_size_with(engine, o, axis == V, |c, s| {
            if !shown(engine, c) {
                return s.minimum;
            }
            let size = s.preferred.max(0.0);
            let least = s.minimum.clamp(0.0, size);
            let given = gives(engine, c, axis, Asks { size, least }, budget);
            size - given.min(size - least)
        });
        // One that places its children as a positioner, or has not
        // measured, keeps them at their size.
        return least.map_or(0.0, |l| (own.size - l).max(0.0));
    }
    if is_a::<Row>(engine, o)
        || is_a::<Column>(engine, o)
        || is_a::<Grid>(engine, o)
        || is_a::<Flow>(engine, o)
    {
        return 0.0;
    }
    let mut widest: f64 = 0.0;
    let mut floor: f64 = 0.0;
    for c in child_items(engine, o) {
        if !shown(engine, c) {
            continue;
        }
        let size = match inset_in_parent(engine, c, axis == V) {
            Some(inset) => own.size - inset,
            None => number(engine, c, IMPLICIT[axis]),
        }
        .max(0.0);
        let child = Asks { size, least: 0.0 };
        let given = gives(engine, c, axis, child, budget).min(size);
        widest = widest.max(size);
        floor = floor.max(size - given);
    }
    (widest - floor).max(0.0)
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
            && !content.is_some_and(|c| content_scrolls(engine, popup, c, over, down))
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
