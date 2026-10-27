//! Putting the scene at a point of the matrix and back.
//!
//! A point goes in through what a host would change: the system settings
//! (`SystemPrefs`) and the size of the window. The scene is then settled
//! as a host would settle it, frame after frame until nothing is waiting,
//! with reduced motion on so that what a change starts (a theme's colour
//! animation, a popup moving) ends at once instead of being audited half
//! way. Afterwards everything is put back exactly as it was, held
//! settings and all; an animation that was running when the audit began
//! has ended by then, which is the one thing a full audit changes.

use ljos_a11y::SystemPrefs;
use ljos_controls::{Window, WindowSystem};
use ljos_design::limits::audit::{NARROW_FRACTION, SHORT_FRACTION};
use ljos_engine::{Engine, ObjectId, Value, WriteKind};
use ljos_quick::{HeldPrefs, Item, QuickScene};
use ljos_render::kurbo::Rect;
use ljos_runtime::Time;

use crate::cx::is_a;
use crate::matrix::{MatrixPoint, PointSize};

/// Frames a settle may take before it gives up: a scene that keeps
/// asking for frames (an endless animation) is audited as it is then.
const SETTLE_FRAMES: usize = 32;

/// What the scene was before the audit.
pub(crate) struct Held {
    prefs: HeldPrefs,
    system: SystemPrefs,
    viewport: Rect,
    /// The root's width and height, which a size point writes too.
    root_size: Option<[f64; 2]>,
}

/// A point as the settings and size it gives this scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Concrete {
    prefs: SystemPrefs,
    size: [f64; 2],
}

fn root_size(engine: &Engine, root: ObjectId) -> [f64; 2] {
    engine.untrack(|| {
        [
            engine.read(root, Item::WIDTH.index).to_number(),
            engine.read(root, Item::HEIGHT.index).to_number(),
        ]
    })
}

pub(crate) fn hold(engine: &Engine, scene: &QuickScene) -> Held {
    Held {
        prefs: ljos_quick::hold_system_prefs(engine),
        system: ljos_quick::system_prefs(engine),
        viewport: scene.viewport(),
        root_size: scene.root().map(|r| root_size(engine, r)),
    }
}

impl Held {
    /// The point that changes nothing, as settings and size.
    pub(crate) fn current(&self) -> Concrete {
        Concrete {
            prefs: self.system,
            size: [self.viewport.width(), self.viewport.height()],
        }
    }
}

/// What `point` gives this scene.
pub(crate) fn resolve(
    engine: &Engine,
    scene: &QuickScene,
    held: &Held,
    point: &MatrixPoint,
) -> Concrete {
    let mut c = held.current();
    if point.is_current() {
        return c;
    }
    let p = &mut c.prefs;
    if let Some(s) = point.scheme {
        p.color_scheme = s;
    }
    if let Some(k) = point.contrast {
        p.contrast = k;
    }
    if let Some(f) = point.font_scale {
        p.font_scale = SystemPrefs::sane_font_scale(f);
    }
    // What a change starts ends at once, unless the point is about motion.
    p.reduced_motion = point.reduced_motion.unwrap_or(true);
    match point.size {
        Some(PointSize::Exact(w, h)) => c.size = [w, h],
        Some(PointSize::Small) => {
            let least = |name: &str| {
                scene
                    .root()
                    .filter(|r| is_a::<Window>(engine, *r))
                    .map(|r| engine.untrack(|| engine.read_by_name(r, name)).to_number())
                    .filter(|n| n.is_finite() && *n >= 1.0)
            };
            let [w, h] = c.size;
            c.size = [
                least("minimumWidth").unwrap_or((w * f64::from(NARROW_FRACTION)).round()),
                least("minimumHeight").unwrap_or((h * f64::from(SHORT_FRACTION)).round()),
            ];
        }
        None => {}
    }
    c
}

/// Frames until the scene waits for nothing at `now`.
pub(crate) fn settle(engine: &Engine, scene: &mut QuickScene, now: Time) {
    for _ in 0..SETTLE_FRAMES {
        let out = scene.frame(engine, now);
        if !scene.needs_frame() && out.next_wake.is_none_or(|t| t > now) {
            return;
        }
    }
}

fn resize(engine: &Engine, scene: &mut QuickScene, size: [f64; 2], root: Option<[f64; 2]>) {
    let v = scene.viewport();
    scene.set_viewport(Rect::new(v.x0, v.y0, v.x0 + size[0], v.y0 + size[1]));
    let (Some(r), Some(root_to)) = (scene.root(), root) else {
        return;
    };
    if is_a::<Window>(engine, r) {
        // As the platform says it: the window does not ask to be resized
        // back.
        WindowSystem::of(engine).resized(engine, r, root_to);
        return;
    }
    // As a viewer sizes its root, but a binding on the root's size stays
    // and is there again afterwards.
    engine.batch(|| {
        for (prop, n) in [
            (Item::WIDTH.index, root_to[0]),
            (Item::HEIGHT.index, root_to[1]),
        ] {
            if engine.untrack(|| engine.read(r, prop)).to_number() != n {
                let _ = engine.write(r, prop, Value::Real(n), WriteKind::Layout);
            }
        }
    });
}

/// Puts the scene at `to`.
pub(crate) fn apply(
    engine: &Engine,
    scene: &mut QuickScene,
    now: Time,
    held: &Held,
    to: &Concrete,
) {
    ljos_quick::set_system_prefs(engine, to.prefs);
    // A root that fills its window is sized with it, as a viewer sizes
    // it; one that is smaller keeps its size.
    let [vw, vh] = held.current().size;
    let fills = held
        .root_size
        .is_some_and(|[w, h]| (w - vw).abs() < 1.0 && (h - vh).abs() < 1.0);
    let root = if fills { Some(to.size) } else { held.root_size };
    resize(engine, scene, to.size, root);
    settle(engine, scene, now);
}

/// Puts back what [`hold`] kept: the settings and size first with motion
/// still reduced, so that what the change back starts ends at once, and
/// then the motion setting as it was.
pub(crate) fn restore(engine: &Engine, scene: &mut QuickScene, now: Time, held: Held) {
    let mut still = held.system;
    still.reduced_motion = true;
    ljos_quick::set_system_prefs(engine, still);
    let v = held.viewport;
    resize(engine, scene, [v.width(), v.height()], held.root_size);
    scene.set_viewport(v);
    settle(engine, scene, now);
    ljos_quick::restore_system_prefs(engine, held.prefs);
    settle(engine, scene, now);
}
