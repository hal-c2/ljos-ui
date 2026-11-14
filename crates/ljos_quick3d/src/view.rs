//! `View3D`: the item a scene is shown in.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use ljos::item::kurbo::{Point, Rect, Size};
use ljos::item::ljos_3d::SceneSlot;
use ljos::item::ljos_3d::qml::{self, Piece, Read, SceneSync, Synced};
use ljos::item::scene3d::View3dOp;
use ljos::item::{DisplayListBuilder, ItemPaint, TextEngine};
use ljos::ljos_engine::NodeId as Effect;
use ljos::prelude::*;
use slotmap::Key;

use crate::source::EngineSource;

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
    /// The root of the view's scene, which the nodes declared in the view
    /// are in, for what asks after a scene (a `PhysicsWorld`) and to make
    /// nodes in at run time. There from the view's completion on.
    #[qml(notify, readonly, object = "Node")]
    pub scene: Option<ObjectId>,
    /// Kept: every mode is drawn in paint order, as `Offscreen` is.
    #[qml(notify, enumeration = "RenderMode")]
    pub render_mode: i32,
    /// The numbers of how this scene is drawn, made when first read.
    #[qml(notify, readonly, object = "RenderStats")]
    pub render_stats: Option<ObjectId>,
    /// The scene last painted, kept while nothing it was read from
    /// changes so that its picture is not drawn again.
    #[qml(skip)]
    last: RefCell<SceneSlot>,
    /// The scene, kept and patched where its objects change.
    #[qml(skip)]
    sync: Rc<Kept>,
}

/// A view's scene and the tracking of each of its pieces: an object's
/// change reads that object again and patches its nodes, not the scene.
#[derive(Default)]
struct Kept {
    scene: RefCell<SceneSync<ObjectId>>,
    /// Each piece's reads, told rather than run when they change.
    effects: RefCell<HashMap<Piece<ObjectId>, Effect>>,
    /// Pieces whose reads changed since the last paint.
    dirty: RefCell<Vec<Piece<ObjectId>>>,
    /// Reads the tracking ran and the paint has not taken yet.
    fresh: RefCell<HashMap<Piece<ObjectId>, Read<ObjectId>>>,
    /// While the paint reads: a run then is one it asked for.
    syncing: Cell<bool>,
    /// A paint was asked for and has not come yet.
    asked: Cell<bool>,
}

impl fmt::Debug for Kept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Kept")
            .field("pieces", &self.effects.borrow().len())
            .field("dirty", &self.dirty.borrow().len())
            .finish()
    }
}

impl Kept {
    /// Has the view painted again, once until it is.
    fn ask(&self, e: &Engine, view: ObjectId) {
        if !self.asked.replace(true) {
            ljos::item::repaint(e, view);
        }
    }

    /// Stops tracking the pieces no longer in the view.
    fn forget(&self, e: &Engine, gone: &[Piece<ObjectId>]) {
        if gone.is_empty() {
            return;
        }
        let mut effects = self.effects.borrow_mut();
        let gone: Vec<Effect> = gone.iter().filter_map(|p| effects.remove(p)).collect();
        drop(effects);
        e.graph().dispose_all(&gone);
    }

    /// The read of `piece`: the one its tracking ran, or one run now. A
    /// piece seen for the first time is tracked from now on.
    fn read(self: &Rc<Self>, e: &Engine, view: ObjectId, piece: Piece<ObjectId>) -> Read<ObjectId> {
        if let Some(r) = self.fresh.borrow_mut().remove(&piece) {
            return r;
        }
        let known = self.effects.borrow().get(&piece).copied();
        match known.filter(|n| e.graph().is_alive(*n)) {
            Some(n) => e.graph().run_effect_now(n),
            None => {
                let run = Rc::downgrade(self);
                let told = Rc::downgrade(self);
                let weak = e.downgrade();
                let made = e.effect_told_in(
                    view,
                    move |e| {
                        let Some(kept) = run.upgrade() else {
                            return;
                        };
                        let r = qml::read(&EngineSource::new(e), view, piece);
                        kept.fresh.borrow_mut().insert(piece, r);
                        // Run by the graph itself (a scope thawed, a
                        // source went): the paint takes it.
                        if !kept.syncing.get() {
                            kept.ask(e, view);
                        }
                    },
                    move || {
                        if let (Some(kept), Some(e)) = (told.upgrade(), weak.upgrade()) {
                            kept.dirty.borrow_mut().push(piece);
                            kept.ask(&e, view);
                        }
                    },
                );
                if let Ok(n) = made {
                    self.effects.borrow_mut().insert(piece, n);
                    // Run at once only when the graph was idle.
                    if !self.fresh.borrow().contains_key(&piece) {
                        e.graph().run_effect_now(n);
                    }
                }
            }
        }
        // Untracked: what the paint itself reads paints the whole scene
        // again.
        self.fresh
            .borrow_mut()
            .remove(&piece)
            .unwrap_or_else(|| e.untrack(|| qml::read(&EngineSource::new(e), view, piece)))
    }
}

#[derive(Debug)]
struct Picture(Arc<View3dOp>);

impl ItemPaint for Picture {
    fn paint(&self, size: Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        out.view3d(Rect::from_origin_size(Point::ORIGIN, size), self.0.clone());
    }
}

/// Reads the view's scene: the pieces of it whose reads changed since the
/// last paint, each tracked apart, so a change paints the view again and
/// is read alone.
pub(crate) fn read(e: &Engine, me: ObjectId) -> Rc<dyn ItemPaint> {
    use ljos::item::ljos_3d::stats;
    let clock = stats::Stopwatch::start();
    let synced = sync(e, me);
    stats::add_objects_read(synced.read as u64);
    // The view's own tracking paints it again, so nobody listens: the
    // slot only keeps the scene it drew while nothing changed.
    let scene = e
        .with(me, |v: &View3D| {
            v.last.borrow_mut().offer_shared(synced.scene)
        })
        .unwrap_or_default();
    // Reading the scene is renderStats' syncTime, the next frame's.
    stats::add_sync_seconds(clock.seconds());
    Rc::new(Picture(Arc::new(View3dOp {
        key: me.data().as_ffi(),
        scene,
    })))
}

/// Brings the view's kept scene up to date with the pieces that changed.
pub(crate) fn sync(e: &Engine, me: ObjectId) -> Synced<ObjectId> {
    // Only a View3D is painted by this, so the read does not fail.
    let kept = e.with(me, |v: &View3D| v.sync.clone()).unwrap_or_default();
    kept.syncing.set(true);
    kept.asked.set(false);
    let pending = || {
        let mut changed = std::mem::take(&mut *kept.dirty.borrow_mut());
        changed.extend(kept.fresh.borrow().keys().copied());
        changed
    };
    let mut synced = kept
        .scene
        .borrow_mut()
        .sync(pending(), |piece| kept.read(e, me, piece));
    kept.forget(e, &synced.gone);
    // A read can run what was waiting in the graph (a LodManager that
    // follows the camera just read), whose writes tell other pieces while
    // the paint is under way: they are patched in this paint, not left
    // for one that may show a scene half of which changed. A loop of
    // reads that keep changing each other is cut short and painted again.
    for _ in 0..PASSES {
        let changed = pending();
        if changed.is_empty() {
            break;
        }
        let more = kept
            .scene
            .borrow_mut()
            .sync(changed, |piece| kept.read(e, me, piece));
        // Each pass's: a piece gone in one may be back in the next.
        kept.forget(e, &more.gone);
        synced.read += more.read;
        synced.changed |= more.changed;
        synced.gone.extend(more.gone);
        synced.scene = more.scene;
    }
    kept.syncing.set(false);
    let quiet = kept.dirty.borrow().is_empty() && kept.fresh.borrow().is_empty();
    if !quiet {
        kept.asked.set(false);
        kept.ask(e, me);
    }
    #[cfg(debug_assertions)]
    if quiet && let Some(drift) = e.untrack(|| kept.scene.borrow().drift(&EngineSource::new(e), me))
    {
        panic!("the scene patched is not the one built anew: patched, then built: {drift}");
    }
    synced
}

/// How many times a paint patches again what its own reads changed.
const PASSES: usize = 8;

/// The view's size in logical pixels: what a point of it is measured in.
pub(crate) fn size(e: &Engine, view: ObjectId) -> (f32, f32) {
    let w = ljos::item::prop(e, view, Item::WIDTH);
    let h = ljos::item::prop(e, view, Item::HEIGHT);
    (w as f32, h as f32)
}
