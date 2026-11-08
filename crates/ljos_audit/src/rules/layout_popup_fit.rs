//! `layout-popup-fit`: a popup far larger than what it holds, or too
//! small for it.
//!
//! Each open popup's size against its implicit size, which is what its
//! content, header, footer and padding ask for: filled to less than
//! `limits::POPUP_MIN_FILL` of its height it is too large (a width is
//! often one menus and dialogs share, so it is not held to its content);
//! asked for more than it has on a side it is too small when something it
//! holds is out of reach, and of no size at all it hides what it holds. A
//! popup reaching past the window is too large for it. Tool tips size
//! themselves and are left out, and a drawer is not held to its fill: it
//! runs along an edge of the window by design, whatever it holds.
//!
//! Out of reach is read from the scene as it is laid out, not worked out
//! again here: layouts, anchors, loaders and positioners have already put
//! every item where it is. What the popup holds is in reach when the
//! popup shows it, inside its padding and what clips it, or when a
//! scroller around it can bring it there: one the pointer, the wheel or
//! a scroll bar moves along that axis, as far as its content goes.

use ljos_controls::{Drawer, ToolTip, item_popup, scrolled_by_bar};
use ljos_design::limits::POPUP_MIN_FILL;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{Text, TextEdit, TextInput, child_items, flickable};
use ljos_render::kurbo::{Point, Rect};

use crate::cx::{Cx, is_a, number, overhang, overhang_words, scene_rect, shown_items, slack, trim};

/// Where along an axis something can be seen, in the scene: from the
/// first to the second. Nowhere when the first is past the second.
type Span = [f64; 2];

const NOWHERE: Span = [f64::INFINITY, f64::NEG_INFINITY];

fn meet(a: Span, b: Span) -> Span {
    [a[0].max(b[0]), a[1].min(b[1])]
}

fn hull(a: Span, b: Span) -> Span {
    [a[0].min(b[0]), a[1].max(b[1])]
}

fn spans(r: Rect) -> [Span; 2] {
    [[r.x0, r.x1], [r.y0, r.y1]]
}

/// Something the popup holds that cannot be seen however it is scrolled:
/// a shown item drawn past what reaches it along some axis.
fn out_of_reach(cx: &Cx, popup: ObjectId, frame: ObjectId, content: ObjectId) -> Option<ObjectId> {
    let engine = cx.engine;
    // What the popup shows of its content: its box inside its padding,
    // mapped as the popup is drawn, scaled and all.
    let (w, h) = (
        number(engine, popup, "width"),
        number(engine, popup, "height"),
    );
    let pad = |name| number(engine, popup, name).max(0.0);
    let corners = [
        Point::new(pad("leftPadding"), pad("topPadding")),
        Point::new(w - pad("rightPadding"), h - pad("bottomPadding")),
    ]
    .map(|p| ljos_quick::map_to_scene(engine, frame, p));
    let shows = spans(Rect::from_points(corners[0], corners[1]));
    let mut budget = crate::cx::DEPTH;
    let mut stack = vec![(content, shows)];
    while let Some((o, reach)) = stack.pop() {
        budget = budget.checked_sub(1)?;
        if !shown(engine, o) || cx.scene.is_culled(o) {
            continue;
        }
        let Some(rect) = scene_rect(engine, o) else {
            continue;
        };
        if let Some(drawn) = drawn(engine, o, rect)
            && (0..2).any(|axis| {
                drawn[axis][0] < reach[axis][0] - slack()
                    || drawn[axis][1] > reach[axis][1] + slack()
            })
        {
            return Some(o);
        }
        let inner = reach_inside(engine, o, rect, reach);
        stack.extend(child_items(engine, o).into_iter().map(|c| (c, inner)));
    }
    None
}

/// Where an item draws along each axis: its box, and past it what it
/// needs that it was not given. Text draws all its lines however small
/// its box (`contentWidth` and `contentHeight` inside its padding); an
/// item that holds nothing shown tells what it needs by its implicit size
/// alone. A line of input scrolls itself to its cursor, so it draws in
/// its box. `None` for an item that neither has nor needs a length on
/// some axis: it draws nothing, and what it holds is measured on its own.
fn drawn(engine: &Engine, o: ObjectId, rect: Rect) -> Option<[Span; 2]> {
    let size = [number(engine, o, "width"), number(engine, o, "height")];
    let n = |name| number(engine, o, name);
    let needs = if is_a::<Text>(engine, o) || is_a::<TextEdit>(engine, o) {
        [
            n("contentWidth") + n("leftPadding") + n("rightPadding"),
            n("contentHeight") + n("topPadding") + n("bottomPadding"),
        ]
    } else if is_a::<TextInput>(engine, o)
        || child_items(engine, o).into_iter().any(|c| shown(engine, c))
    {
        [0.0, 0.0]
    } else {
        [n("implicitWidth"), n("implicitHeight")]
    };
    let mut out = spans(rect);
    for (axis, span) in out.iter_mut().enumerate() {
        let length = size[axis].max(needs[axis]);
        if length.is_nan() || length <= 0.0 {
            return None;
        }
        // In the scene's pixels, as the item is drawn there.
        let shown_as = span[1] - span[0];
        let scale = if size[axis] > 0.0 && shown_as > 0.0 {
            shown_as / size[axis]
        } else {
            1.0
        };
        span[1] = span[1].max(span[0] + length * scale);
    }
    Some(out)
}

/// Where what is inside `o` can be seen, given where `o` itself can: no
/// further than its box if it clips, and as far as a scroller brings its
/// content into the part of it that is seen, on an axis it scrolls along.
fn reach_inside(engine: &Engine, o: ObjectId, rect: Rect, reach: [Span; 2]) -> [Span; 2] {
    let clips = engine
        .untrack(|| engine.read_by_name(o, "clip"))
        .to_boolean();
    let own = spans(rect);
    let scroller = flickable::is_flickable(engine, o);
    let mut out = reach;
    for (axis, span) in out.iter_mut().enumerate() {
        let horizontal = axis == 0;
        let seen = meet(reach[axis], own[axis]);
        let along = if scroller && scrolls(engine, o, horizontal) {
            if seen[0] > seen[1] {
                NOWHERE
            } else {
                // The content moves by as much as the scroller has left to
                // scroll before and after where it is now.
                let at = engine.untrack(|| flickable::axis_state(engine, o, horizontal));
                [
                    seen[0] - (at.position - at.min),
                    seen[1] + (at.max - at.position),
                ]
            }
        } else {
            seen
        };
        *span = if clips {
            along
        } else {
            hull(along, reach[axis])
        };
    }
    out
}

/// Whether the user can move what a Flickable holds along an axis: the
/// pointer and the wheel do where it lets them, and a scroll bar does,
/// whether attached to it or to the view it is the content of.
fn scrolls(engine: &Engine, f: ObjectId, horizontal: bool) -> bool {
    flickable::can_scroll(engine, f, horizontal) || scrolled_by_bar(engine, f, horizontal)
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
        let asks_more = cw - aw > slack() || ch - ah > slack();
        if asks_more && content.is_none_or(|c| out_of_reach(cx, popup, item, c).is_some()) {
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
