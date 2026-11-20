//! `Back` (Ljos): where the system's Back goes (ARCHITECTURE.md §10.3,
//! "the `Ljos.Back` router"). Qt has no such type.
//!
//! Every `Back` of an engine is in one list, in the order they were made
//! (not completed: a component completes the last made first). The
//! system's Back goes to the last one that is enabled and whose item
//! is shown: a page pushed on another, a popup over a page. While none is,
//! Back is the platform's to do ([`wants_back`] tells a host so, ahead of
//! time, which a phone needs to play its own animation for it).
//!
//! A swipe (Android's predictive back) is told as it goes: [`BackStep::
//! Started`] picks the handler, and the rest of the swipe goes to that one
//! whatever is enabled meanwhile, as the platform previews the one it
//! started with. A Back with no swipe before it (a key, a button) is a
//! lone [`BackStep::Invoked`].
//!
//! Pinned by `features/mobile/back.feature`.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};

use ljos_engine::qml_impl;
use ljos_engine::{Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Value, WriteKind};

use crate::item::{Item, is_item};

/// `Back` (Ljos).
#[derive(Debug, QmlObject)]
#[qml(module = "Ljos", name = "Back", extends = QtObject)]
pub struct Back {
    #[qml(base)]
    pub object: QtObject,
    /// Whether it takes Back now. A handler that has nowhere to go back to
    /// says so here, so that Back is the platform's.
    #[qml(notify)]
    pub enabled: bool,
    /// A swipe for this handler is under way.
    #[qml(notify, readonly)]
    pub active: bool,
    /// How far the swipe has come, from 0 to 1; 0 when there is none.
    #[qml(notify, readonly)]
    pub progress: f64,
    /// When it was made, among every `Back`: the router's order.
    #[qml(skip)]
    made: u64,
}

/// The next `Back`'s [`Back::made`].
fn next_made() -> u64 {
    static MADE: AtomicU64 = AtomicU64::new(0);
    MADE.fetch_add(1, Ordering::Relaxed)
}

impl Default for Back {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            enabled: true,
            active: false,
            progress: 0.0,
            made: next_made(),
        }
    }
}

#[qml_impl]
impl Back {
    /// A swipe started; `progress` is 0.
    #[qml(signal)]
    fn started();
    /// The swipe moved; `progress` says how far it has come.
    #[qml(signal)]
    fn progressed();
    /// The swipe was let go short: nothing goes back.
    #[qml(signal)]
    fn cancelled();
    /// Go back.
    #[qml(signal)]
    fn invoked();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let made = cx.engine.with_mut(cx.this, |b: &mut Back| b.made)?;
        let router = cx.engine.extension::<Router>();
        let mut all = router.all.borrow_mut();
        let at = all.partition_point(|(m, _)| *m < made);
        all.insert(at, (made, cx.this));
        Ok(())
    }
}

/// One step of the system's Back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackStep {
    /// A swipe started.
    Started,
    /// The swipe moved.
    Progressed,
    /// The swipe was let go short.
    Cancelled,
    /// Go back: a swipe completed, or a key or button with no swipe.
    Invoked,
}

#[derive(Default)]
struct Router {
    /// Every completed `Back` with when it was made, in that order; the
    /// dead ones are dropped as Back is next looked for.
    all: RefCell<Vec<(u64, ObjectId)>>,
    /// The handler a swipe under way started at.
    swiping: Cell<Option<ObjectId>>,
}

fn flag(engine: &Engine, o: ObjectId, p: ljos_engine::PropIndex) -> bool {
    matches!(engine.read(o, p), Value::Bool(true))
}

/// Whether the item a handler is declared in, and the items it is in, are
/// shown. A handler in no item is shown. An owner that is not an item but
/// says whether it is visible (a `Popup` the handler is declared in)
/// hides it as an item does.
fn shown(engine: &Engine, handler: ObjectId) -> bool {
    let mut cur = engine.owner(handler);
    // Bounded so a corrupt parent cycle cannot hang.
    for _ in 0..4096 {
        match cur {
            Some(o) if is_item(engine, o) => {
                if !flag(engine, o, Item::VISIBLE.index) {
                    return false;
                }
                cur = engine.visual_parent(o).or_else(|| engine.owner(o));
            }
            Some(o) => {
                if matches!(engine.read_by_name(o, "visible"), Value::Bool(false)) {
                    return false;
                }
                cur = engine.owner(o);
            }
            None => return true,
        }
    }
    true
}

/// The handler Back goes to now: the last made one that is enabled
/// and shown.
fn topmost(engine: &Engine) -> Option<ObjectId> {
    let router = engine.extension::<Router>();
    router.all.borrow_mut().retain(|(_, o)| engine.is_alive(*o));
    let all = router.all.borrow().clone();
    all.into_iter()
        .rev()
        .map(|(_, b)| b)
        .find(|b| flag(engine, *b, Back::ENABLED.index) && shown(engine, *b))
}

/// Whether the application takes Back now, for a host to tell the
/// platform before the user swipes (Android plays its own animation only
/// for an application that does not take Back). Not a tracked read: a
/// handler made later adds itself to no property, so an effect would miss
/// it. A host asks again each time its loop is about to wait, after
/// whatever could have made or changed a handler.
pub fn wants_back(engine: &Engine) -> bool {
    topmost(engine).is_some()
}

fn set(engine: &Engine, b: ObjectId, active: bool, progress: f64) {
    let _ = engine.write_internal(
        b,
        Back::ACTIVE.index,
        Value::Bool(active),
        WriteKind::Imperative,
    );
    let _ = engine.write_internal(
        b,
        Back::PROGRESS.index,
        Value::Real(progress),
        WriteKind::Imperative,
    );
}

/// Hands one step of the system's Back to the application, `progress`
/// (0 to 1) being how far a swipe has come. Whether the application took
/// it: when it did not, Back is the platform's to do.
pub fn deliver_back(engine: &Engine, step: BackStep, progress: f64) -> bool {
    let progress = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let router = engine.extension::<Router>();
    let target = engine.untrack(|| match step {
        BackStep::Started => {
            let t = topmost(engine);
            router.swiping.set(t);
            t
        }
        BackStep::Progressed => router.swiping.get(),
        BackStep::Cancelled => router.swiping.take(),
        // A swipe ends where it started, unless that handler is gone; a
        // lone Back goes to the topmost.
        BackStep::Invoked => router
            .swiping
            .take()
            .filter(|b| engine.is_alive(*b))
            .or_else(|| topmost(engine)),
    });
    let Some(b) = target.filter(|b| engine.is_alive(*b)) else {
        return false;
    };
    let signal = match step {
        BackStep::Started => {
            set(engine, b, true, 0.0);
            "started"
        }
        BackStep::Progressed => {
            set(engine, b, true, progress);
            "progressed"
        }
        BackStep::Cancelled => {
            set(engine, b, false, 0.0);
            "cancelled"
        }
        BackStep::Invoked => {
            set(engine, b, false, 0.0);
            "invoked"
        }
    };
    let _ = engine.emit_by_name(b, signal, &[]);
    engine.request_wake();
    true
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("Ljos" 1.0 => [Back]));
}
