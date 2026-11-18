//! An application that puts another root in the window of the one it
//! shows ([`replace_root`]). The application asks from anywhere it holds
//! the engine, a signal handler of the old root included; the host that
//! shows the root makes the change before its next frame, when nothing of
//! the old root runs.

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{Engine, ObjectFlags, ObjectId};

use crate::Item;
use crate::item::is_item;

/// Why [`replace_root`] refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RootError {
    #[error("{0:?} is gone")]
    Gone(ObjectId),
    #[error("{0:?} is not a root that a window shows")]
    NotShown(ObjectId),
    #[error("{0:?} is shown already")]
    Shown(ObjectId),
    #[error("{0:?} is not an Item, which a window shows")]
    NotAnItem(ObjectId),
}

/// A root a host is to show in place of one it shows.
#[derive(Debug)]
struct Swap {
    old: ObjectId,
    new: ObjectId,
    /// Destroyed once `new` is shown.
    retire: Vec<ObjectId>,
    /// Destroyed when the window cannot show `new`.
    refused: Option<ObjectId>,
    /// `new` takes the size `old` has first: the window keeps its own.
    adopt: bool,
}

/// The roots the hosts of an engine show, and the replacements asked for
/// and not yet made.
///
/// A root may stand for a window it holds ([`present`](Self::present)):
/// a host asked to show the root shows that window, in the platform
/// window the root would have had, and the root lives on unshown. That
/// is how a `ShellLoader` at the root of an application whose shell is a
/// `Window` makes that window the application's.
#[derive(Default)]
pub struct RootSwaps {
    shown: RefCell<Vec<ObjectId>>,
    pending: RefCell<Vec<Swap>>,
    /// Each root that stands for a window, and the window.
    presents: RefCell<Vec<(ObjectId, ObjectId)>>,
    wake: RefCell<Option<Rc<dyn Fn()>>>,
}

impl std::fmt::Debug for RootSwaps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RootSwaps")
            .field("shown", &self.shown.borrow())
            .field("pending", &self.pending.borrow())
            .field("presents", &self.presents.borrow())
            .finish_non_exhaustive()
    }
}

impl RootSwaps {
    /// The one of `engine`.
    pub fn of(engine: &Engine) -> Rc<Self> {
        engine.extension::<Self>()
    }

    /// A host shows `root` in a window of its own; `old`, when given, is
    /// the root it showed there before. A root that stands for a window
    /// is replaced by it before the next frame, at the size `old` had
    /// (at its own when there was none).
    pub fn shows(&self, engine: &Engine, old: Option<ObjectId>, root: ObjectId) {
        {
            let mut shown = self.shown.borrow_mut();
            shown.retain(|r| Some(*r) != old && *r != root);
            shown.push(root);
        }
        self.prune(engine);
        if self.present_now(engine, root, old.is_some()) {
            self.wake();
        }
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

    /// Whether a replacement waits to be made in the window of `root`.
    pub fn is_pending_for(&self, root: ObjectId) -> bool {
        self.pending.borrow().iter().any(|s| s.old == root)
    }

    /// The window `holder` stands for, if it stands for one.
    pub fn presented(&self, holder: ObjectId) -> Option<ObjectId> {
        let presents = self.presents.borrow();
        presents.iter().find(|(h, _)| *h == holder).map(|(_, w)| *w)
    }

    /// The root that stands for `window`, if one does: the root of the
    /// application still, though a host shows `window` in its place.
    pub fn holder_of(&self, window: ObjectId) -> Option<ObjectId> {
        let presents = self.presents.borrow();
        presents.iter().find(|(_, w)| *w == window).map(|(h, _)| *h)
    }

    /// `holder` stands for `window` from now on (None: for nothing, and
    /// it is shown itself). Where a host shows the holder, or the window
    /// it stood for, the host shows what it stands for now before the
    /// next frame. True when the window it stood for is no longer the
    /// holder's to destroy: it goes once the host has let go of it.
    pub fn present(&self, engine: &Engine, holder: ObjectId, window: Option<ObjectId>) -> bool {
        self.prune(engine);
        let before = {
            let mut presents = self.presents.borrow_mut();
            let before = presents
                .iter()
                .position(|(h, _)| *h == holder)
                .map(|at| presents.remove(at).1);
            if let Some(w) = window {
                presents.push((holder, w));
            }
            before
        };
        let target = window.unwrap_or(holder);
        let current = {
            let mut shown = self.shown.borrow_mut();
            let Some(at) = shown
                .iter()
                .position(|r| *r == holder || Some(*r) == before)
            else {
                // No host shows it: its windows are its own business.
                return false;
            };
            let current = std::mem::replace(&mut shown[at], target);
            if current == target {
                return false;
            }
            current
        };
        // A window the holder let go of; never the holder itself.
        let let_go = current != holder;
        let mut pending = self.pending.borrow_mut();
        match pending.iter().position(|s| s.new == current) {
            // `current` never got the window.
            Some(at) => {
                if pending[at].old == target {
                    pending.remove(at);
                } else {
                    pending[at].new = target;
                }
                drop(pending);
                if let_go {
                    engine.destroy(current);
                }
            }
            None => {
                pending.push(Swap {
                    old: current,
                    new: target,
                    retire: if let_go { vec![current] } else { Vec::new() },
                    refused: None,
                    adopt: true,
                });
                drop(pending);
            }
        }
        self.wake();
        let_go
    }

    /// Shows in the window of `root` what was asked to go there, as
    /// often as was asked: each time [`adopt`] where it is to, `show`
    /// puts `new` in the window of `old` (false: it cannot, and `old`
    /// stays), and what `new` replaces is destroyed. The root shown in
    /// the end, and the roots destroyed on the way, for a host that keeps
    /// its own list of them.
    pub fn swap_in(
        &self,
        engine: &Engine,
        root: ObjectId,
        show: &mut dyn FnMut(ObjectId, ObjectId) -> bool,
    ) -> (ObjectId, Vec<ObjectId>) {
        let mut root = root;
        let mut gone = Vec::new();
        loop {
            let swap = {
                let mut pending = self.pending.borrow_mut();
                let Some(at) = pending.iter().position(|s| s.old == root) else {
                    break;
                };
                pending.remove(at)
            };
            // One destroyed since it was asked for is dropped, and `root`
            // stays.
            if !lasts(engine, swap.new) {
                self.restore(swap.new, root);
                continue;
            }
            if swap.adopt {
                adopt(engine, root, swap.new);
            }
            if show(root, swap.new) {
                for r in swap.retire {
                    engine.destroy(r);
                    gone.push(r);
                }
                root = swap.new;
                self.present_now(engine, root, true);
            } else {
                self.restore(swap.new, root);
                if let Some(n) = swap.refused {
                    engine.destroy(n);
                }
            }
        }
        (root, gone)
    }

    /// The window shows `old` again in place of `new`.
    fn restore(&self, new: ObjectId, old: ObjectId) {
        let mut shown = self.shown.borrow_mut();
        shown.retain(|r| *r != old);
        if let Some(at) = shown.iter().position(|r| *r == new) {
            shown[at] = old;
        } else {
            shown.push(old);
        }
    }

    /// Asks for the window `root` stands for in place of `root`, which a
    /// host shows. True when it was asked for.
    fn present_now(&self, engine: &Engine, root: ObjectId, adopt: bool) -> bool {
        let Some(window) = self.presented(root).filter(|w| lasts(engine, *w)) else {
            return false;
        };
        {
            let mut shown = self.shown.borrow_mut();
            if shown.contains(&window) {
                return false;
            }
            let Some(at) = shown.iter().position(|r| *r == root) else {
                return false;
            };
            shown[at] = window;
        }
        self.pending.borrow_mut().push(Swap {
            old: root,
            new: window,
            retire: Vec::new(),
            refused: None,
            adopt,
        });
        true
    }

    fn prune(&self, engine: &Engine) {
        self.presents
            .borrow_mut()
            .retain(|(h, w)| engine.is_alive(*h) && engine.is_alive(*w));
    }

    fn wake(&self) {
        let wake = self.wake.borrow().clone();
        if let Some(wake) = wake {
            wake();
        }
    }

    fn ask(&self, engine: &Engine, old: ObjectId, new: ObjectId) -> Result<(), RootError> {
        for (o, alive) in [(old, engine.is_alive(old)), (new, lasts(engine, new))] {
            if !alive {
                return Err(RootError::Gone(o));
            }
        }
        if !is_item(engine, new) {
            return Err(RootError::NotAnItem(new));
        }
        self.prune(engine);
        // `old` may be a root that stands for the window shown, or that
        // window: the root goes, and the window with it.
        let holder = match self.presented(old) {
            Some(_) => Some(old),
            None => {
                let presents = self.presents.borrow();
                presents.iter().find(|(_, w)| *w == old).map(|(h, _)| *h)
            }
        };
        let face = {
            let mut shown = self.shown.borrow_mut();
            if shown.contains(&new) {
                return Err(RootError::Shown(new));
            }
            let window = holder.and_then(|h| self.presented(h));
            let at = shown
                .iter()
                .position(|r| *r == old || Some(*r) == holder || Some(*r) == window)
                .ok_or(RootError::NotShown(old))?;
            // From now on `new` is the one to ask about, before the host
            // has made the change: a handler may hand the window on at once.
            std::mem::replace(&mut shown[at], new)
        };
        let goes = holder.unwrap_or(old);
        let mut pending = self.pending.borrow_mut();
        match pending.iter_mut().find(|s| s.new == face) {
            // `face` never got the window: `new` takes the place it was to have.
            Some(waiting) => {
                let skipped = std::mem::replace(&mut waiting.new, new);
                if let Some(h) = holder
                    && !waiting.retire.contains(&h)
                {
                    waiting.retire.push(h);
                }
                waiting.refused = Some(new);
                waiting.adopt = true;
                drop(pending);
                if Some(skipped) != holder {
                    engine.destroy(skipped);
                }
            }
            None => {
                pending.push(Swap {
                    old: face,
                    new,
                    retire: vec![goes],
                    refused: Some(new),
                    adopt: true,
                });
                drop(pending);
            }
        }
        self.wake();
        Ok(())
    }
}

/// Puts `new` in the window that shows the root `old`, at the size that
/// window has, before the next frame; `old` is destroyed then. `new` is a
/// root the application made with no parent, as `old` was: a `Window`
/// takes the platform window `old` had, and a plain item fills it.
///
/// # Errors
/// `old` or `new` is gone, `new` is not an `Item`, `old` is no root a
/// window shows, or `new` is shown already.
pub fn replace_root(engine: &Engine, old: ObjectId, new: ObjectId) -> Result<(), RootError> {
    RootSwaps::of(engine).ask(engine, old, new)
}

/// Whether `obj` is alive and not to be destroyed: a root that is goes
/// from the window as soon as it is shown.
pub(crate) fn lasts(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .flags(obj)
        .is_some_and(|f| !f.contains(ObjectFlags::PENDING_DELETE))
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
