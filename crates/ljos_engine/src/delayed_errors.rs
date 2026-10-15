//! What a binding says while its object is being made waits for the
//! creation to be complete.
//!
//! A binding often reads something only `Component.onCompleted` sets.
//! Its first run fails, and the warning would be noise if it then runs
//! again without one. So the warnings of a binding run during a creation
//! are held, keyed by the property, and replaced or forgotten by its next
//! run. Those still held once the creation's `onCompleted` handlers have
//! run are logged. A creation made inside another one hands what it holds
//! to the outer one; a creation in slices holds across its slices.

use std::cell::{Cell, RefCell};

use crate::engine::Engine;
use crate::ids::{ObjectId, PropIndex};

#[derive(Default)]
pub(crate) struct DelayedErrors {
    next: Cell<u64>,
    /// The creations running now, the innermost last.
    open: RefCell<Vec<u64>>,
    /// Where warnings go instead of the log while a held binding runs.
    capture: RefCell<Option<Vec<String>>>,
    held: RefCell<Vec<Held>>,
}

struct Held {
    creation: u64,
    obj: ObjectId,
    prop: PropIndex,
    said: Vec<String>,
}

/// Puts the outer capture back, also when the binding unwinds.
struct Capture<'a> {
    cell: &'a RefCell<Option<Vec<String>>>,
    outer: Option<Option<Vec<String>>>,
}

impl Capture<'_> {
    fn end(mut self) -> Vec<String> {
        let outer = self.outer.take().flatten();
        self.cell.replace(outer).unwrap_or_default()
    }
}

impl Drop for Capture<'_> {
    fn drop(&mut self) {
        if let Some(outer) = self.outer.take() {
            self.cell.replace(outer);
        }
    }
}

/// A creation whose bindings' warnings are held (see the module).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CreationErrors(u64);

impl Engine {
    /// Starts a creation; [`Engine::close_creation`] ends it.
    pub(crate) fn open_creation(&self) -> CreationErrors {
        let d = &self.inner.delayed;
        let id = d.next.get() + 1;
        d.next.set(id);
        d.open.borrow_mut().push(id);
        CreationErrors(id)
    }

    /// A creation in slices carries on in this one.
    pub(crate) fn resume_creation(&self, c: CreationErrors) {
        self.inner.delayed.open.borrow_mut().push(c.0);
    }

    /// The slice of a creation in slices ends; what it holds stays held.
    pub(crate) fn pause_creation(&self, c: CreationErrors) {
        let mut open = self.inner.delayed.open.borrow_mut();
        if let Some(i) = open.iter().rposition(|&o| o == c.0) {
            open.remove(i);
        }
    }

    /// The creation is complete: what its bindings still say is logged,
    /// or held by the creation around it.
    pub(crate) fn close_creation(&self, c: CreationErrors) {
        self.pause_creation(c);
        let d = &self.inner.delayed;
        let outer = d.open.borrow().last().copied();
        let said: Vec<String> = {
            let mut held = d.held.borrow_mut();
            if let Some(outer) = outer {
                for h in held.iter_mut().filter(|h| h.creation == c.0) {
                    h.creation = outer;
                }
                return;
            }
            let (mine, rest) = std::mem::take(&mut *held)
                .into_iter()
                .partition::<Vec<_>, _>(|h| h.creation == c.0);
            *held = rest;
            mine.into_iter().flat_map(|h| h.said).collect()
        };
        for msg in said {
            self.warn(msg);
        }
    }

    /// Runs the binding on `prop` of `obj`, holding what it says while a
    /// creation is running or an earlier run of it is still held.
    pub(crate) fn run_binding<R>(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        f: impl FnOnce() -> R,
    ) -> R {
        let d = &self.inner.delayed;
        let earlier = d
            .held
            .borrow()
            .iter()
            .find(|h| h.obj == obj && h.prop == prop)
            .map(|h| h.creation);
        let Some(creation) = earlier.or_else(|| d.open.borrow().last().copied()) else {
            return f();
        };
        let guard = Capture {
            cell: &d.capture,
            outer: Some(d.capture.replace(Some(Vec::new()))),
        };
        let out = f();
        let said = guard.end();
        let mut held = d.held.borrow_mut();
        held.retain(|h| h.obj != obj || h.prop != prop);
        if !said.is_empty() {
            held.push(Held {
                creation,
                obj,
                prop,
                said,
            });
        }
        out
    }

    /// Takes a warning that a held binding run says; false otherwise.
    pub(crate) fn hold_warning(&self, msg: &str) -> bool {
        match self.inner.delayed.capture.borrow_mut().as_mut() {
            Some(said) => {
                said.push(msg.to_owned());
                true
            }
            None => false,
        }
    }
}
