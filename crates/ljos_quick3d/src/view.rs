//! `View3D`: the item a scene is shown in.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ljos::item::kurbo::{Point, Rect, Size};
use ljos::item::ljos_3d::Scene;
use ljos::item::scene3d::View3dOp;
use ljos::item::{DisplayListBuilder, ItemPaint, TextEngine};
use ljos::prelude::*;
use slotmap::Key;

fn view_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "RenderMode",
        vec![
            ("Offscreen", 0),
            ("Underlay", 1),
            ("Overlay", 2),
            ("Inline", 3),
        ],
    )]
}

/// `View3D`: shows the nodes declared in it, through `camera` (or the
/// first camera in the scene), against `environment`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D", name = "View3D", extends = Item, enums = view_enums)]
pub struct View3D {
    #[qml(base)]
    pub item: Item,
    #[qml(notify, object = "Camera")]
    pub camera: Option<ObjectId>,
    #[qml(notify, object = "SceneEnvironment")]
    pub environment: Option<ObjectId>,
    /// Another tree of nodes shown as well as this view's own.
    #[qml(notify, object = "Node")]
    pub import_scene: Option<ObjectId>,
    /// Kept: every mode is drawn in paint order, as `Offscreen` is.
    #[qml(notify, enumeration = "RenderMode")]
    pub render_mode: i32,
    /// The scene last painted, kept while a walk comes out the same so
    /// that its picture is not drawn again.
    #[qml(skip)]
    last: RefCell<Option<Arc<Scene>>>,
}

#[derive(Debug)]
struct Picture(Arc<View3dOp>);

impl ItemPaint for Picture {
    fn paint(&self, size: Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        out.view3d(Rect::from_origin_size(Point::ORIGIN, size), self.0.clone());
    }
}

/// Reads the view's scene. Tracked: any property of a node, material or
/// environment the walk reads paints the view again when it changes.
pub(crate) fn read(e: &Engine, me: ObjectId) -> Rc<dyn ItemPaint> {
    let scene = crate::scene::build(e, me);
    let kept = e
        .with(me, |v: &View3D| v.last.borrow().clone())
        .ok()
        .flatten();
    let scene = match kept {
        Some(prev) if prev.same_as(&scene) => prev,
        _ => {
            let scene = Arc::new(scene);
            let _ = e.with(me, |v: &View3D| *v.last.borrow_mut() = Some(scene.clone()));
            scene
        }
    };
    Rc::new(Picture(Arc::new(View3dOp {
        key: me.data().as_ffi(),
        scene,
    })))
}
