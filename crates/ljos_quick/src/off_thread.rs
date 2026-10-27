//! Work handed to the compute pool from the engine's thread.

use std::sync::{Arc, Mutex, PoisonError};

use ljos_runtime::{ComputePool, Lane};

type Work = Box<dyn FnOnce() + Send>;

/// Runs `work` on the compute pool, or here when the pool is full: late
/// beats never. The pool keeps a job it refuses, so the job sits in a slot
/// both sides can reach and whichever comes first takes it.
pub(crate) fn off_thread(work: impl FnOnce() + Send + 'static) {
    let slot: Arc<Mutex<Option<Work>>> = Arc::new(Mutex::new(Some(Box::new(work))));
    let run = |slot: &Mutex<Option<Work>>| {
        let work = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(work) = work {
            work();
        }
    };
    let for_pool = Arc::clone(&slot);
    if ComputePool::global()
        .spawn(Lane::Visible, move || run(&for_pool))
        .is_err()
    {
        run(&slot);
    }
}
