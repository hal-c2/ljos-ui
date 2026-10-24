//! An application that puts another root in the window of the one it
//! shows ([`replace_root`]). The application asks from anywhere it holds
//! the engine, a signal handler of the old root included; the host that
//! shows the root makes the change before its next frame, when nothing of
//! the old root runs.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId};

use crate::Item;

/// Why [`replace_root`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RootError {
    #[error("{0:?} is gone")]
    Gone(ObjectId),
    #[error("{0:?} is not a root that a window shows")]
    NotShown(ObjectId),
    #[error("{0:?} is shown already")]
    Shown(ObjectId),
}

/// The roots the hosts of an engine show, and the replacements asked for
/// and not yet made.
#[derive(Default)]
pub struct RootSwaps {
    shown: RefCell<Vec<ObjectId>>,
    pending: RefCell<Vec<(ObjectId, ObjectId)>>,
    wake: RefCell<Option<Rc<dyn Fn()>>>,
}

impl std::fmt::Debug for RootSwaps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootSwaps")
            .field("shown", &self.shown.borrow())
            .field("pending", &self.pending.borrow())
            .finish_non_exhaustive()
    }
}

impl RootSwaps {
    /// The one of `engine`.
    pub fn of(engine: &Engine) -> Rc<Self> {
        engine.extension::<Self>()
    }

    /// A host shows `root` in a window of its own; `old`, when given, is
    /// the root it showed there before.
    pub fn shows(&self, old: Option<ObjectId>, root: ObjectId) {
        let mut shown = self.shown.borrow_mut();
        shown.retain(|r| Some(*r) != old && *r != root);
        shown.push(root);
    }

    /// The host no longer shows `root`.
    pub fn hides(&self, root: ObjectId) {
        self.shown.borrow_mut().retain(|r| *r != root);
    }

    /// Called when a replacement is asked for, for the host to make it
    /// before its next frame.
    pub fn set_wake(&self, wake: impl Fn() + 'static) {
        *self.wake.borrow_mut() = Some(Rc::new(wake));
    }

    /// Whether a replacement waits to be made.
    pub fn is_pending(&self) -> bool {
        !self.pending.borrow().is_empty()
    }

    /// The root asked to go in the window of `root`, if one was. The host
    /// calls [`adopt`] and puts it in the window, then destroys `root`.
    pub fn take(&self, root: ObjectId) -> Option<ObjectId> {
        let mut pending = self.pending.borrow_mut();
        let at = pending.iter().position(|(old, _)| *old == root)?;
        Some(pending.remove(at).1)
    }

    fn ask(&self, engine: &Engine, old: ObjectId, new: ObjectId) -> Result<(), RootError> {
        for o in [old, new] {
            if !engine.is_alive(o) {
                return Err(RootError::Gone(o));
            }
        }
        {
            let mut shown = self.shown.borrow_mut();
            if shown.contains(&new) {
                return Err(RootError::Shown(new));
            }
            let at = shown
                .iter()
                .position(|r| *r == old)
                .ok_or(RootError::NotShown(old))?;
            // From now on `new` is the one to ask about, before the host
            // has made the change: a handler may hand the window on at once.
            shown[at] = new;
        }
        let mut pending = self.pending.borrow_mut();
        match pending.iter_mut().find(|(_, n)| *n == old) {
            // `old` never got the window: `new` takes the place it was to have.
            Some(waiting) => {
                let skipped = std::mem::replace(&mut waiting.1, new);
                drop(pending);
                engine.destroy(skipped);
            }
            None => {
                pending.push((old, new));
                drop(pending);
            }
        }
        let wake = self.wake.borrow().clone();
        if let Some(wake) = wake {
            wake();
        }
        Ok(())
    }
}

/// Puts `new` in the window that shows the root `old`, at the size that
/// window has, before the next frame; `old` is destroyed then. `new` is a
/// root the application made with no parent, as `old` was: a `Window`
/// takes the platform window `old` had, and a plain item fills it.
///
/// # Errors
/// `old` or `new` is gone, `old` is no root a window shows, or `new` is
/// shown already.
pub fn replace_root(engine: &Engine, old: ObjectId, new: ObjectId) -> Result<(), RootError> {
    RootSwaps::of(engine).ask(engine, old, new)
}

/// What a host does before it shows `new` where `old` was: `new` takes the
/// size `old` has, which is the window's, since the window stays.
pub fn adopt(engine: &Engine, old: ObjectId, new: ObjectId) {
    engine.batch(|| {
        for size in [Item::WIDTH, Item::HEIGHT] {
            if let Ok(v) = engine.get(old, size) {
                let _ = engine.set(new, size, v);
            }
        }
    });
}
