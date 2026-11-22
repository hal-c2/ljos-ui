//! When the first read of a shell that was made is over, for what is held
//! until then: a `ShellLoader`'s fallback.
//!
//! Bindings are lazy, so a shell that was made may still fail as it is
//! first read. Who reads it is judged when that could be over, by where
//! the shell is then:
//!
//! - a visible `Window` of its own, which its host has not refused: the
//!   first frame of that window;
//! - one a scene shows, or that waits to be put in a window: the first
//!   frame of that scene or window;
//! - any other: the first frame of any scene, or the first delivery of
//!   faults outside every frame.
//!
//! A frame counts if its polish began after the hold. Each is over once
//! the faults filed by then are delivered. Nothing polls: with nothing
//! held a frame does no work for this, and what is held waits for events.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId};

use crate::roots::RootSwaps;
use crate::scene::Scenes;

/// What a holder does once the read is over, given the hold's number.
type Over = fn(&Engine, ObjectId, u64);

#[derive(Debug)]
struct Hold {
    n: u64,
    holder: ObjectId,
    shell: ObjectId,
    /// The held item, where it is an item of a scene: it is left out of
    /// what is described to assistive technology.
    out: Option<ObjectId>,
    refused: bool,
    over: Over,
}

/// Who ends a hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reader {
    /// The frames of the scene whose root this is.
    Root(ObjectId),
    /// The frames of the scenes that show either of these.
    Showing(ObjectId, ObjectId),
    Any,
}

/// What an engine holds through first reads.
#[derive(Debug, Default)]
pub(crate) struct FirstReads {
    holds: RefCell<Vec<Hold>>,
    /// Numbers holds and the starts of polish, in one order.
    count: Cell<u64>,
    /// The frames under way.
    frames: Cell<u32>,
}

/// A frame under way: a delivery in one ends no read.
pub(crate) struct InFrame(Rc<FirstReads>);

impl Drop for InFrame {
    fn drop(&mut self) {
        self.0.frames.set(self.0.frames.get() - 1);
    }
}

impl InFrame {
    pub(crate) fn enter(e: &Engine) -> Self {
        let reads = e.extension::<FirstReads>();
        reads.frames.set(reads.frames.get() + 1);
        Self(reads)
    }

    /// The frame begins polish: the mark to give [`framed`](Self::framed).
    pub(crate) fn polishes(&self) -> u64 {
        self.0.next()
    }

    /// The frame of the scene that shows `root`, whose polish began at
    /// `mark`, has read what it reads.
    pub(crate) fn framed(&self, e: &Engine, root: Option<ObjectId>, mark: u64) {
        if self.0.holds.borrow().is_empty() {
            return;
        }
        self.0.holds.borrow_mut().retain(|h| e.is_alive(h.holder));
        let begun: Vec<_> = (self.0.holds.borrow().iter())
            .filter(|h| h.n < mark)
            .map(|h| (h.n, h.holder, h.shell, h.refused))
            .collect();
        for (n, holder, shell, refused) in begun {
            let ends = match reader(e, holder, shell, refused) {
                Reader::Root(top) => root == Some(top),
                Reader::Showing(a, b) => root.is_some_and(|root| {
                    crate::live_input::is_in(e, a, root) || crate::live_input::is_in(e, b, root)
                }),
                Reader::Any => true,
            };
            if ends {
                self.0.read(e, n);
            }
        }
    }
}

impl FirstReads {
    fn next(&self) -> u64 {
        let n = self.count.get() + 1;
        self.count.set(n);
        n
    }

    /// The read of hold `n` is done: it is over once its faults are told.
    fn read(&self, e: &Engine, n: u64) {
        if e.has_pending_faults() {
            e.after_faults_delivered(move |e| e.extension::<FirstReads>().over(e, n));
        } else {
            self.over(e, n);
        }
    }

    fn over(&self, e: &Engine, n: u64) {
        let hold = {
            let mut holds = self.holds.borrow_mut();
            let Some(at) = holds.iter().position(|h| h.n == n) else {
                // It ended meanwhile, as by one of those faults.
                return;
            };
            holds.remove(at)
        };
        if e.is_alive(hold.holder) {
            // Not under a borrow: it runs the application's handlers.
            (hold.over)(e, hold.holder, n);
        }
    }
}

/// Holds, for `holder`, until the first read of `shell` is over; `over`
/// is called then, unless the hold was [`end`]ed. `out` is the held item
/// where it is one of a scene. The hold's number.
pub(crate) fn hold(
    e: &Engine,
    holder: ObjectId,
    shell: ObjectId,
    out: Option<ObjectId>,
    over: Over,
) -> u64 {
    let reads = e.extension::<FirstReads>();
    let n = reads.next();
    reads.holds.borrow_mut().push(Hold {
        n,
        holder,
        shell,
        out,
        refused: false,
        over,
    });
    e.after_faults_delivered(move |e| delivered(e, n));
    n
}

/// Hold `n` is over with, and its `over` is not to be called.
pub(crate) fn end(e: &Engine, n: u64) {
    let reads = e.extension::<FirstReads>();
    reads.holds.borrow_mut().retain(|h| h.n != n);
}

/// The host will draw no frame of `window`.
pub(crate) fn refuse(e: &Engine, window: ObjectId) {
    let reads = e.extension::<FirstReads>();
    for h in reads.holds.borrow_mut().iter_mut() {
        if h.shell == window {
            h.refused = true;
        }
    }
}

/// The held items that are not described to assistive technology.
pub(crate) fn left_out(e: &Engine) -> Vec<ObjectId> {
    let reads = e.extension::<FirstReads>();
    let holds = reads.holds.borrow();
    holds.iter().filter_map(|h| h.out).collect()
}

/// A delivery of faults is done while `n` is held. Outside every frame it
/// is how a host with no scene says that its turn's reads are done.
fn delivered(e: &Engine, n: u64) {
    let reads = e.extension::<FirstReads>();
    let held = (reads.holds.borrow().iter())
        .find(|h| h.n == n)
        .map(|h| (h.holder, h.shell, h.refused));
    let Some((holder, shell, refused)) = held else {
        return;
    };
    if !e.is_alive(holder) {
        end(e, n);
    } else if reads.frames.get() == 0 && reader(e, holder, shell, refused) == Reader::Any {
        reads.read(e, n);
    } else {
        e.after_faults_delivered(move |e| delivered(e, n));
    }
}

fn reader(e: &Engine, holder: ObjectId, shell: ObjectId, refused: bool) -> Reader {
    if crate::blocks::is_own_window(e, holder, shell) {
        let visible = e.untrack(|| e.read_by_name(shell, "visible").to_boolean());
        // Judged before whether a scene shows it: a host makes a window's
        // scene before the window, and frames none it hides or refused.
        return if visible && !refused {
            Reader::Root(shell)
        } else {
            Reader::Any
        };
    }
    // The held window alone is no reach: where a swap is to show the
    // shell, the frame to wait for is the one after it.
    let swaps = RootSwaps::of(e);
    if let Some(root) = [holder, shell].into_iter().find(|o| swaps.expects(*o)) {
        return Reader::Root(root);
    }
    let scenes = e.extension::<Scenes>();
    if scenes.shows(e, holder) || scenes.shows(e, shell) {
        Reader::Showing(holder, shell)
    } else {
        Reader::Any
    }
}
