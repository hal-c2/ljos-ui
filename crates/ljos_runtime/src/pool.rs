//! The compute pool: a few threads for work that must not block the UI thread
//! (image decode, parsing, shaping; ARCHITECTURE.md 3.3).
//!
//! Jobs are plain closures. A job that has something to tell the Engine sends
//! it through a [`UiHandle`](crate::UiHandle), which is also what wakes the UI
//! thread; the pool itself knows nothing about the Engine. The queue is
//! bounded and ordered by [`Lane`], so work for what is on screen overtakes
//! prefetching. Threads are started on demand and park on a condition variable
//! when there is nothing to do: an idle pool costs no CPU.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;

use thiserror::Error;

use crate::lane::Lane;

type Work = Box<dyn FnOnce() + Send + 'static>;

/// The queue of a [`ComputePool`] is full; the job was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("the compute pool already holds {capacity} waiting jobs")]
pub struct PoolFull {
    /// How many jobs the queue holds at most.
    pub capacity: usize,
}

struct Queued {
    lane: Lane,
    seq: u64,
    work: Work,
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.seq == other.seq
    }
}

impl Eq for Queued {}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Queued {
    /// `BinaryHeap` pops the greatest: the most urgent lane, then the job
    /// that has waited longest.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .lane
            .cmp(&self.lane)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

#[derive(Default)]
struct State {
    queue: BinaryHeap<Queued>,
    seq: u64,
    running: usize,
    threads: usize,
    parked: usize,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    /// Signalled when a job is queued or the pool shuts down.
    work: Condvar,
    /// Signalled when the last job finishes.
    idle: Condvar,
    capacity: usize,
    max_threads: usize,
}

impl Shared {
    /// A job that panics is caught before it can poison the lock, so a
    /// poisoned lock still guards consistent state.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A bounded pool of worker threads.
pub struct ComputePool {
    shared: Arc<Shared>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl std::fmt::Debug for ComputePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.shared.lock();
        f.debug_struct("ComputePool")
            .field("threads", &s.threads)
            .field("queued", &s.queue.len())
            .field("running", &s.running)
            .finish()
    }
}

impl ComputePool {
    /// Queue length of [`ComputePool::global`].
    pub const DEFAULT_CAPACITY: usize = 256;

    /// A pool of at most `max_threads` threads (at least one) whose queue
    /// holds at most `capacity` waiting jobs. No thread is started until the
    /// first job arrives.
    pub fn new(max_threads: usize, capacity: usize) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                work: Condvar::new(),
                idle: Condvar::new(),
                capacity: capacity.max(1),
                max_threads: max_threads.max(1),
            }),
            workers: Mutex::new(Vec::new()),
        }
    }

    /// The process-wide pool: two to four threads, by the number of cores.
    pub fn global() -> &'static ComputePool {
        static POOL: OnceLock<ComputePool> = OnceLock::new();
        POOL.get_or_init(|| {
            let cores = std::thread::available_parallelism().map_or(2, usize::from);
            ComputePool::new(cores.saturating_sub(1).clamp(2, 4), Self::DEFAULT_CAPACITY)
        })
    }

    /// The most threads this pool runs.
    pub fn max_threads(&self) -> usize {
        self.shared.max_threads
    }

    /// Jobs waiting or running.
    pub fn pending(&self) -> usize {
        let s = self.shared.lock();
        s.queue.len() + s.running
    }

    /// Queue `work`. It runs on a pool thread, after every waiting job of a
    /// more urgent lane and every earlier job of its own.
    pub fn spawn(&self, lane: Lane, work: impl FnOnce() + Send + 'static) -> Result<(), PoolFull> {
        let start_thread = {
            let mut s = self.shared.lock();
            if s.queue.len() >= self.shared.capacity {
                return Err(PoolFull {
                    capacity: self.shared.capacity,
                });
            }
            s.seq += 1;
            let seq = s.seq;
            s.queue.push(Queued {
                lane,
                seq,
                work: Box::new(work),
            });
            // A parked thread takes the job; otherwise grow up to the limit.
            let grow = s.parked == 0 && s.threads < self.shared.max_threads;
            if grow {
                s.threads += 1;
            }
            grow
        };
        if start_thread {
            let shared = Arc::clone(&self.shared);
            let spawned = std::thread::Builder::new()
                .name("ljos-compute".into())
                .spawn(move || worker(&shared));
            match spawned {
                Ok(handle) => self
                    .workers
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(handle),
                Err(_) => {
                    // The job stays queued for the threads that exist. With
                    // none at all there is nobody to run it: say so.
                    let mut s = self.shared.lock();
                    s.threads -= 1;
                    if s.threads == 0 {
                        s.queue.clear();
                        return Err(PoolFull { capacity: 0 });
                    }
                }
            }
        }
        self.shared.work.notify_one();
        Ok(())
    }

    /// Block until no job is waiting or running. For tests and shutdown; the
    /// UI thread never calls it.
    pub fn wait_idle(&self) {
        let mut s = self.shared.lock();
        while !s.queue.is_empty() || s.running > 0 {
            s = self.shared.idle.wait(s).unwrap_or_else(|e| e.into_inner());
        }
    }
}

fn worker(shared: &Shared) {
    let mut s = shared.lock();
    loop {
        if let Some(job) = s.queue.pop() {
            s.running += 1;
            drop(s);
            // A panicking job must not take the thread with it: the pool
            // would silently shrink. Whoever waits for the job's message sees
            // its sender dropped.
            let _ = catch_unwind(AssertUnwindSafe(job.work));
            s = shared.lock();
            s.running -= 1;
            if s.running == 0 && s.queue.is_empty() {
                shared.idle.notify_all();
            }
        } else if s.shutdown {
            break;
        } else {
            s.parked += 1;
            s = shared.work.wait(s).unwrap_or_else(|e| e.into_inner());
            s.parked -= 1;
        }
    }
    s.threads -= 1;
}

impl Drop for ComputePool {
    /// Waiting jobs still run; then the threads end.
    fn drop(&mut self) {
        self.shared.lock().shutdown = true;
        self.shared.work.notify_all();
        let workers = std::mem::take(&mut *self.workers.lock().unwrap_or_else(|e| e.into_inner()));
        for w in workers {
            let _ = w.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::mpsc;

    use super::*;
    use crate::inbox::{DrainBudget, UiInbox};
    use crate::time::SystemClock;

    #[test]
    fn jobs_run_off_the_calling_thread() {
        let pool = ComputePool::new(2, 16);
        let (tx, rx) = mpsc::channel();
        let here = std::thread::current().id();
        pool.spawn(Lane::Visible, move || {
            tx.send(std::thread::current().id()).unwrap();
        })
        .unwrap();
        assert_ne!(rx.recv().unwrap(), here);
        pool.wait_idle();
        assert_eq!(pool.pending(), 0);
    }

    #[test]
    fn no_thread_exists_before_the_first_job() {
        let pool = ComputePool::new(4, 16);
        assert_eq!(pool.shared.lock().threads, 0);
        pool.spawn(Lane::Visible, || {}).unwrap();
        pool.wait_idle();
        assert_eq!(pool.shared.lock().threads, 1);
    }

    #[test]
    fn threads_never_exceed_the_limit() {
        let pool = ComputePool::new(2, 64);
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let gate_rx = Arc::new(Mutex::new(gate_rx));
        for _ in 0..8 {
            let gate = Arc::clone(&gate_rx);
            pool.spawn(Lane::Visible, move || {
                let _ = gate.lock().unwrap().recv();
            })
            .unwrap();
        }
        assert_eq!(pool.shared.lock().threads, 2);
        assert_eq!(pool.max_threads(), 2);
        for _ in 0..8 {
            gate_tx.send(()).unwrap();
        }
        pool.wait_idle();
    }

    /// Hold the only thread, queue jobs in mixed lanes, then let go.
    #[test]
    fn urgent_lanes_run_first_and_each_lane_is_fifo() {
        let pool = ComputePool::new(1, 16);
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let (started_tx, started_rx) = mpsc::channel::<()>();
        pool.spawn(Lane::Visible, move || {
            started_tx.send(()).unwrap();
            gate_rx.recv().unwrap();
        })
        .unwrap();
        started_rx.recv().unwrap();
        let order = Arc::new(Mutex::new(Vec::new()));
        for (lane, name) in [
            (Lane::Idle, "idle"),
            (Lane::Prefetch, "prefetch-1"),
            (Lane::Visible, "visible-1"),
            (Lane::Prefetch, "prefetch-2"),
            (Lane::Visible, "visible-2"),
        ] {
            let order = Arc::clone(&order);
            pool.spawn(lane, move || order.lock().unwrap().push(name))
                .unwrap();
        }
        gate_tx.send(()).unwrap();
        pool.wait_idle();
        assert_eq!(
            *order.lock().unwrap(),
            ["visible-1", "visible-2", "prefetch-1", "prefetch-2", "idle"]
        );
    }

    #[test]
    fn a_full_queue_refuses_the_job() {
        let pool = ComputePool::new(1, 2);
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let (started_tx, started_rx) = mpsc::channel::<()>();
        pool.spawn(Lane::Visible, move || {
            started_tx.send(()).unwrap();
            gate_rx.recv().unwrap();
        })
        .unwrap();
        started_rx.recv().unwrap();
        pool.spawn(Lane::Visible, || {}).unwrap();
        pool.spawn(Lane::Visible, || {}).unwrap();
        assert_eq!(
            pool.spawn(Lane::Visible, || {}),
            Err(PoolFull { capacity: 2 })
        );
        gate_tx.send(()).unwrap();
        pool.wait_idle();
        // Room again once the queue has drained.
        pool.spawn(Lane::Visible, || {}).unwrap();
        pool.wait_idle();
    }

    #[test]
    fn a_panicking_job_does_not_end_the_thread() {
        let pool = ComputePool::new(1, 8);
        let ran = Arc::new(AtomicUsize::new(0));
        pool.spawn(Lane::Visible, || panic!("job failed")).unwrap();
        let r = Arc::clone(&ran);
        pool.spawn(Lane::Visible, move || {
            r.fetch_add(1, AtomicOrdering::SeqCst);
        })
        .unwrap();
        pool.wait_idle();
        assert_eq!(ran.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(pool.shared.lock().threads, 1);
    }

    #[test]
    fn dropping_the_pool_finishes_the_queue() {
        let ran = Arc::new(AtomicUsize::new(0));
        {
            let pool = ComputePool::new(2, 64);
            for _ in 0..20 {
                let r = Arc::clone(&ran);
                pool.spawn(Lane::Deferred, move || {
                    r.fetch_add(1, AtomicOrdering::SeqCst);
                })
                .unwrap();
            }
        }
        assert_eq!(ran.load(AtomicOrdering::SeqCst), 20);
    }

    /// The intended use: the result travels through a `UiHandle`, whose wake
    /// hook is the only thing that tells the UI thread about it.
    #[test]
    fn results_come_back_through_the_inbox_with_one_wake() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let w = Arc::clone(&wakes);
        let (inbox, handle) = UiInbox::<u32>::channel_with_wake(
            8,
            Arc::new(move || {
                w.fetch_add(1, AtomicOrdering::SeqCst);
            }),
        );
        let pool = ComputePool::new(2, 8);
        pool.spawn(Lane::Visible, move || {
            handle.send(6 * 7).unwrap();
        })
        .unwrap();
        pool.wait_idle();
        assert_eq!(wakes.load(AtomicOrdering::SeqCst), 1);
        let mut got = Vec::new();
        inbox.drain(DrainBudget::UNLIMITED, &SystemClock::new(), |m| got.push(m));
        assert_eq!(got, [42]);
    }

    #[test]
    fn the_global_pool_has_two_to_four_threads() {
        let pool = ComputePool::global();
        assert!((2..=4).contains(&pool.max_threads()));
        assert!(std::ptr::eq(pool, ComputePool::global()));
    }
}
