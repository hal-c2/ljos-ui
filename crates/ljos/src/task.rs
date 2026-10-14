//! Background work: a closure on the compute pool, its value back on
//! the Engine.
//!
//! Heavy work never runs on the UI thread; it answers through the
//! inbox. This is that shape, the same one `Platform::fetch` has:
//! [`spawn`] queues `work` on the process-wide bounded compute pool,
//! and when the host next drains the platform inbox (the scene does
//! at the start of a frame; tests call `Platform::finish`), `done`
//! runs on the Engine thread with the value `work` made. Nothing
//! polls: no delivery happens until the host delivers, as with fetch.
//!
//! Write the two closures to their threads:
//!
//! - Capture plain data into `work` — strings, numbers, bytes. It
//!   must be `Send` and `'static`, so it cannot reach the Engine or
//!   a QML object even by accident. Read what QML knows before
//!   spawning, and carry copies.
//! - Touch QML only in `done`. It runs where QML state may be
//!   touched, never on the pool thread, and it need not be `Send`:
//!   it stays on the Engine thread from spawn to delivery, so it may
//!   hold the `Rc`s an Engine-thread callback usually holds.
//! - When a newer request supersedes an older one (a search box, a
//!   reload), cancel the old [`Task`] if you hold it and still guard
//!   a generation in `done`: a result already delivered cannot be
//!   cancelled, and the guard is what makes a late answer harmless.
//!
//! ```
//! use std::cell::{Cell, RefCell};
//! use std::rc::Rc;
//!
//! let engine = ljos::engine();
//! // The generation of the newest request, and what the UI shows.
//! let current = Rc::new(Cell::new(0u64));
//! let shown = Rc::new(RefCell::new(String::new()));
//!
//! let generation = current.get() + 1;
//! current.set(generation);
//! let latest = Rc::clone(&current);
//! let put = Rc::clone(&shown);
//! let query = String::from("ljos"); // plain data crosses to the pool
//! ljos::task::spawn(
//!     &engine,
//!     move || format!("{query}: {} hits", 2),
//!     move |_engine, text| {
//!         if latest.get() != generation {
//!             return; // a newer request superseded this one
//!         }
//!         *put.borrow_mut() = text; // QML state is touched here
//!     },
//! )
//! .expect("queued");
//!
//! // A host delivers at the start of a frame; `finish` is the
//! // harness's way of saying "deliver everything, now".
//! ljos::prelude::Platform::of(&engine).finish(&engine);
//! assert_eq!(shown.borrow().as_str(), "ljos: 2 hits");
//! ```
//!
//! The functions live behind this module (`ljos::task::spawn`); the
//! prelude names only the [`Task`] handle, so the surface a native
//! type writes against stays small.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ljos_engine::Engine;
use ljos_platform::{Platform, Reply};
use ljos_runtime::ComputePool;

pub use ljos_runtime::{Lane, PoolFull};

/// The handle of one spawned task: how its requester cancels it.
///
/// Cancelling before the work starts means it never runs; cancelling
/// while it runs lets it finish but discards the result (`done` is
/// never called); cancelling after delivery does nothing. Dropping
/// the handle does *not* cancel — fire-and-forget, `spawn(..)?;`
/// with the handle unbound, is a legitimate shape, and the result is
/// still delivered.
#[derive(Debug)]
pub struct Task {
    cancelled: Arc<AtomicBool>,
}

impl Task {
    /// Asks that the result never be delivered, and that the work
    /// never start if it has not. Running work is not interrupted.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// A `done` waiting for its value, type-erased over that value: the
/// closure stays here, on the Engine thread, and only the value its
/// work made crosses back through the inbox. That is why `done`
/// need not be `Send` where the inbox's jobs must be — the same
/// reason `Platform::fetch` keeps its callbacks engine-side.
type Done = Box<dyn FnOnce(&Engine, Box<dyn Any + Send>)>;

/// The `done`s of one engine's tasks that have not been delivered.
#[derive(Default)]
struct Callbacks {
    next: Cell<u64>,
    pending: RefCell<HashMap<u64, Done>>,
}

impl Callbacks {
    fn take(&self, id: u64) -> Option<Done> {
        self.pending.borrow_mut().remove(&id)
    }
}

fn callbacks(engine: &Engine) -> std::rc::Rc<Callbacks> {
    engine.extension::<Callbacks>()
}

/// Answers the promise without delivering: the stored `done` is
/// dropped on the Engine thread, where it was made.
fn discard(reply: Reply, id: u64) {
    reply.send(move |engine| {
        drop(callbacks(engine).take(id));
    });
}

/// Runs `work` on the compute pool and, when the host delivers the
/// answer, runs `done` on the Engine thread with its value. In the
/// [`Lane::Deferred`] lane; [`spawn_in`] names another. A full pool
/// queue is [`PoolFull`] and neither closure runs.
///
/// Call it on the Engine thread: `done` is kept there, with the
/// promise of its answer.
pub fn spawn<T, F, D>(engine: &Engine, work: F, done: D) -> Result<Task, PoolFull>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
    D: FnOnce(&Engine, T) + 'static,
{
    spawn_in(engine, Lane::Deferred, work, done)
}

/// [`spawn`] in the lane named: work for what is on screen
/// ([`Lane::Visible`]) overtakes what can wait.
pub fn spawn_in<T, F, D>(engine: &Engine, lane: Lane, work: F, done: D) -> Result<Task, PoolFull>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
    D: FnOnce(&Engine, T) + 'static,
{
    let cancelled = Arc::new(AtomicBool::new(false));
    let waiting = callbacks(engine);
    let id = waiting.next.get();
    waiting.next.set(id + 1);
    waiting.pending.borrow_mut().insert(
        id,
        Box::new(move |engine: &Engine, value: Box<dyn Any + Send>| {
            let value = value
                .downcast::<T>()
                .unwrap_or_else(|_| unreachable!("a task delivers the value its own work made"));
            done(engine, *value);
        }),
    );
    let reply = Platform::of(engine).expect();
    let flag = Arc::clone(&cancelled);
    let queued = ComputePool::global().spawn(lane, move || {
        // First thing on the pool thread: a task cancelled while it
        // waited in the queue never runs its work.
        if flag.load(Ordering::SeqCst) {
            discard(reply, id);
            return;
        }
        // Caught here so a panicking work still drops its `done` on
        // the Engine thread; the pool would otherwise only drop the
        // promise, and the stored closure would outlive its answer.
        let value = match catch_unwind(AssertUnwindSafe(work)) {
            Ok(value) => value,
            Err(panic) => {
                discard(reply, id);
                resume_unwind(panic);
            }
        };
        if flag.load(Ordering::SeqCst) {
            discard(reply, id);
            return;
        }
        reply.send(move |engine| {
            let done = callbacks(engine).take(id);
            // Once more at delivery: a cancel that landed while the
            // answer waited in the inbox still discards it.
            if flag.load(Ordering::SeqCst) {
                return;
            }
            if let Some(done) = done {
                done(engine, Box::new(value));
            }
        });
    });
    match queued {
        Ok(()) => Ok(Task { cancelled }),
        Err(full) => {
            // The pool dropped the closure unrun, promise and all,
            // which counts the promise; the `done` is this thread's
            // to drop.
            drop(waiting.take(id));
            Err(full)
        }
    }
}
