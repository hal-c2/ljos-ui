//! `layout-shift`: something shown moves once time has passed (a load, a
//! timer, a status line that appears), so a user already aiming at it
//! presses what took its place.
//!
//! Compared with where the scene had it when it first came to rest
//! ([`Placement`]). What moves with its parent moves as one: the part
//! that moved is reported once, at the outermost item that moved by that
//! much. When an earlier sibling in a row, column or layout appeared,
//! went or changed size, that sibling pushed it and is reported instead.
//! Left out: what a popup holds (it is placed as it opens), what a view
//! that scrolled carries, and a scene with no earlier placement.

use std::collections::HashMap;

use ljos_controls::item_popup;
use ljos_design::limits::LAYOUT_SHIFT_MIN_PX;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{Column, Flickable, Flow, Grid, Layout, Row};
use ljos_render::kurbo::{Rect, Vec2};

use crate::cx::{Cx, acted_on, ancestors, is_a, scene_rect, shown_items, slack, trim};
use crate::shift::Placement;

fn least() -> f64 {
    f64::from(LAYOUT_SHIFT_MIN_PX)
}

fn far(d: Vec2) -> bool {
    d.x.abs().max(d.y.abs()) >= least()
}

/// Whether an item placed by its parent's flow (a row, a column, a
/// layout) is pushed by what comes before it.
fn flows(e: &Engine, parent: ObjectId) -> bool {
    is_a::<Row>(e, parent)
        || is_a::<Column>(e, parent)
        || is_a::<Flow>(e, parent)
        || is_a::<Grid>(e, parent)
        || is_a::<Layout>(e, parent)
}

struct Look<'a> {
    e: &'a Engine,
    before: &'a Placement,
    now: HashMap<ObjectId, Rect>,
}

impl Look<'_> {
    fn moved(&self, item: ObjectId) -> Option<Vec2> {
        let (a, b) = (self.before.rects.get(&item)?, self.now.get(&item)?);
        Some(b.origin() - a.origin())
    }

    /// It came, went, or grew or shrank.
    fn changed(&self, item: ObjectId) -> bool {
        match (self.before.rects.get(&item), self.now.get(&item)) {
            (Some(a), Some(b)) => {
                (a.width() - b.width()).abs() >= least()
                    || (a.height() - b.height()).abs() >= least()
            }
            (None, None) => false,
            _ => true,
        }
    }

    /// In a popup, or carried by a view that scrolled.
    fn left_out(&self, item: ObjectId) -> bool {
        std::iter::once(item)
            .chain(ancestors(self.e, item))
            .any(|a| {
                item_popup(self.e, a).is_some()
                    || (is_a::<Flickable>(self.e, a) && a != item && {
                        let now = self.e.untrack(|| {
                            self.e
                                .with::<Flickable, _>(a, |f| (f.content_x, f.content_y))
                                .ok()
                        });
                        now != self.before.scrolled(a)
                    })
            })
    }

    /// The outermost item that moved by `d` with `item`.
    fn unit(&self, item: ObjectId, d: Vec2) -> ObjectId {
        let mut unit = item;
        for a in ancestors(self.e, item) {
            match self.moved(a) {
                Some(m) if (m - d).hypot() <= slack() => unit = a,
                _ => break,
            }
        }
        unit
    }

    /// An earlier sibling in a flow that came, went or changed size, at
    /// the unit's level or above.
    fn cause(&self, unit: ObjectId) -> Option<ObjectId> {
        for level in std::iter::once(unit).chain(ancestors(self.e, unit)) {
            let parent = self.e.visual_parent(level)?;
            if !flows(self.e, parent) {
                continue;
            }
            let earlier = self
                .e
                .visual_children(parent)
                .into_iter()
                .take_while(|s| *s != level)
                .filter(|s| self.changed(*s))
                .last();
            if earlier.is_some() {
                return earlier;
            }
        }
        None
    }
}

pub(crate) fn check(cx: &mut Cx) {
    let Some(before) = cx.options.before.clone() else {
        return;
    };
    let e = cx.engine;
    let shown = shown_items(cx);
    let look = Look {
        e,
        before: &before,
        now: shown
            .iter()
            .filter_map(|i| scene_rect(e, *i).map(|r| (*i, r)))
            .collect(),
    };
    let mut units = Vec::new();
    let mut reported = Vec::new();
    for item in shown.iter().copied() {
        let node = cx
            .scene
            .accessible_node(item)
            .and_then(|k| cx.scene.accessibility_tree().get(k).cloned());
        let content = node.as_ref().is_some_and(acted_on)
            || super::controls::text_of(e, item).is_some_and(|t| !t.trim().is_empty());
        if !content {
            continue;
        }
        let Some(d) = look.moved(item).filter(|d| far(*d)) else {
            continue;
        };
        if look.left_out(item) {
            continue;
        }
        let unit = look.unit(item, d);
        if units.contains(&unit) {
            continue;
        }
        units.push(unit);
        let cause = look.cause(unit);
        let at = cause.unwrap_or(unit);
        if reported.contains(&at) {
            continue;
        }
        reported.push(at);
        let message = match cause {
            Some(_) => format!(
                "this came, went or changed size after the scene was shown, and pushed {} \
                 {} px; reserve its room up front (a placeholder of its final size, \
                 `opacity` rather than `visible`), or show it over the content",
                crate::cx::item_words(e, unit),
                trim(d.hypot())
            ),
            None => format!(
                "this moved {} px after the scene was shown, under a pointer that may \
                 already be aiming at it; place it where it ends up from the start, and \
                 show what is loading in its place",
                trim(d.hypot())
            ),
        };
        cx.report("layout-shift", Some(at), message).measured = Some(format!(
            "moved {} px, {} across, {} down",
            trim(d.hypot()),
            trim(d.x),
            trim(d.y)
        ));
    }
}
