//! A view's objects, walked into an `ljos_3d` scene by the mapping the
//! web shares (`ljos_3d::qml`), reading the engine through
//! [`EngineSource`].

use ljos::item::ljos_3d::{self, qml};
use ljos::prelude::*;

use crate::source::EngineSource;

/// A view's scene, and which object each of its nodes is.
pub(crate) type Built = qml::Built<ObjectId>;

/// The scene of the view `view`, read now. Tracked.
pub(crate) fn build(e: &Engine, view: ObjectId) -> ljos_3d::Scene {
    build_with_ids(e, view).scene
}

/// [`build`], with the scene node each object became: what a pick needs
/// to say which object it hit.
pub(crate) fn build_with_ids(e: &Engine, view: ObjectId) -> Built {
    qml::build(&EngineSource::new(e), view)
}

/// The camera `obj` is, as the renderer sees it; `None` when it is not one.
pub(crate) fn camera(e: &Engine, obj: ObjectId) -> Option<ljos_3d::Camera> {
    qml::camera(&EngineSource::new(e), obj)
}

pub(crate) fn color(c: Color) -> ljos_3d::Color {
    let [r, g, b, a] = c.to_rgba_f();
    ljos_3d::Color {
        r: r as f32,
        g: g as f32,
        b: b as f32,
        a: a as f32,
    }
}

/// An object list property's live objects; tracked.
pub(crate) fn list(e: &Engine, obj: ObjectId, p: PropIndex) -> Vec<ObjectId> {
    match e.read(obj, p) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| e.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}
