//! Where the scene had its items before application time passed, for
//! `layout-shift`: what moved since is what a user, already aiming at it,
//! saw jump.
//!
//! The audit sees one moment, so the caller takes a [`Placement`] once
//! the scene first comes to rest, lets its time pass (a load, a timer)
//! and hands the placement to the audit in [`Options::before`]. Nothing
//! is watched in between: two looks, on the caller's clock.
//!
//! [`Options::before`]: crate::Options::before

use std::collections::HashMap;

use ljos_engine::{Engine, ObjectId};
use ljos_quick::{Flickable, QuickScene};
use ljos_render::kurbo::Rect;

use crate::cx::{Cx, scene_rect, shown_items};

/// The rectangle of every item the scene showed, and how far each view
/// that scrolls was scrolled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placement {
    pub(crate) rects: HashMap<ObjectId, Rect>,
    pub(crate) scrolled: HashMap<ObjectId, (f64, f64)>,
}

impl Placement {
    /// The scene as it is now: lay it out and draw it first.
    pub fn of(engine: &Engine, scene: &QuickScene) -> Self {
        let options = crate::Options::desktop();
        let cx = Cx::new(engine, scene, &options);
        let mut out = Placement::default();
        for item in shown_items(&cx) {
            if let Some(r) = scene_rect(engine, item) {
                out.rects.insert(item, r);
            }
            if let Ok(at) =
                engine.untrack(|| engine.with::<Flickable, _>(item, |f| (f.content_x, f.content_y)))
            {
                out.scrolled.insert(item, at);
            }
        }
        out
    }

    /// How far the view was scrolled; `None` when it was not shown.
    pub(crate) fn scrolled(&self, view: ObjectId) -> Option<(f64, f64)> {
        self.scrolled.get(&view).copied()
    }
}
