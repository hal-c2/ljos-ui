//! Bounded, coalescing mailbox into the UI thread (ARCHITECTURE.md 3.3).
//!
//! Any thread or tokio task holds a [`UiHandle`]; the UI thread owns the single
//! [`UiInbox`]. The payload type `M` is a parameter so the engine defines its
//! own message enum (`Service`, `Model`, `Reload`, ...) without this crate
//! knowing about it.
//!
//! # Why a `std::sync::Mutex`
//!
//! The critical section is a `VecDeque` push or pop and one `HashMap` probe,
//! tens of nanoseconds. The UI thread takes the lock once per message and never
//! while running a handler, so senders are never blocked behind user code. A
//! lock-free queue would not add anything for this hold time and could not
//! support in-place coalescing. Nothing waits on a condvar or polls: the UI
//! thread sleeps in the shell's event loop and the [`WakeHook`] is the only
//! thing that wakes it. A poisoned lock is recovered, because every mutation
//! leaves the queue consistent and a sender's panic must not take the UI down.
//! Replaced or rejected payloads are dropped outside the lock, since `M::drop`
//! is arbitrary code.
//!
//! # Wake protocol
//!
//! The hook fires once per transition from empty to non-empty, decided under
//! the lock, so it is exact: no lost wake-up and no hook call per message. If a
//! budgeted [`UiInbox::drain`] leaves messages behind it re-arms the hook, so a
//! partially drained inbox always schedules another frame.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use thiserror::Error;

use crate::time::Clock;

/// Called from a sender's thread when the inbox goes from empty to non-empty.
/// The shell passes something like `move || ctx.request_repaint()`. It must be
/// cheap and must not block.
pub type WakeHook = Arc<dyn Fn() + Send + Sync>;

/// Identifies a stream of "latest value wins" messages (a progress bar, a
/// scroll position, a model snapshot). Chosen by the sender.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CoalesceKey(pub u64);

impl From<u64> for CoalesceKey {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

/// Why a send failed. The rejected message is handed back.
#[derive(Error)]
pub enum SendError<M> {
    /// The inbox is at capacity. Back-pressure: drop, retry later or coalesce.
    #[error("UI inbox is full")]
    Full(M),
    /// The [`UiInbox`] was dropped. The receiver is gone for good.
    #[error("UI inbox is disconnected")]
    Disconnected(M),
}

impl<M> SendError<M> {
    pub fn into_inner(self) -> M {
        match self {
            Self::Full(m) | Self::Disconnected(m) => m,
        }
    }

    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full(_))
    }

    pub fn is_disconnected(&self) -> bool {
        matches!(self, Self::Disconnected(_))
    }
}

// Manual so `M` need not be `Debug`.
impl<M> fmt::Debug for SendError<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(_) => f.write_str("Full(..)"),
            Self::Disconnected(_) => f.write_str("Disconnected(..)"),
        }
    }
}

struct Slot<M> {
    key: Option<CoalesceKey>,
    msg: M,
}

/// FIFO with O(1) in-place replacement by key. Slots are only ever pushed at
/// the back and popped from the front, so a slot's sequence number minus
/// `front_seq` is its index.
struct Queue<M> {
    slots: VecDeque<Slot<M>>,
    front_seq: u64,
    keyed: HashMap<CoalesceKey, u64>,
    closed: bool,
}

impl<M> Queue<M> {
    fn push_back(&mut self, key: Option<CoalesceKey>, msg: M) {
        if let Some(k) = key {
            self.keyed
                .insert(k, self.front_seq + self.slots.len() as u64);
        }
        self.slots.push_back(Slot { key, msg });
    }

    fn pop_front(&mut self) -> Option<M> {
        let slot = self.slots.pop_front()?;
        self.front_seq += 1;
        if let Some(k) = slot.key {
            self.keyed.remove(&k);
        }
        Some(slot.msg)
    }
}

struct Shared<M> {
    queue: Mutex<Queue<M>>,
    capacity: usize,
    /// Mirrors `Queue::closed` for lock-free `is_closed`.
    closed: AtomicBool,
    wake: OnceLock<WakeHook>,
}

impl<M> Shared<M> {
    fn lock(&self) -> MutexGuard<'_, Queue<M>> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn fire_wake(&self) {
        if let Some(w) = self.wake.get() {
            w();
        }
    }
}

/// Sending side. `Clone + Send + Sync`; works from std threads and tokio tasks
/// alike because sends never block and never `.await`.
pub struct UiHandle<M> {
    shared: Arc<Shared<M>>,
}

impl<M> Clone for UiHandle<M> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<M> fmt::Debug for UiHandle<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiHandle")
            .field("capacity", &self.shared.capacity)
            .field("closed", &self.is_closed())
            .finish()
    }
}

impl<M> UiHandle<M> {
    /// Enqueues `msg`, or fails fast with [`SendError::Full`].
    pub fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.push(None, msg)
    }

    /// Enqueues `msg` under `key`. If a message with this key is still queued
    /// it is replaced in place, so a flood of updates occupies one slot and
    /// never reports `Full`.
    ///
    /// Replacement keeps the *position* of the first message, so a coalesced
    /// message may be handled before a plain message sent after it. Use it only
    /// for idempotent latest-value updates.
    pub fn send_coalesced(&self, key: CoalesceKey, msg: M) -> Result<(), SendError<M>> {
        self.push(Some(key), msg)
    }

    /// True once the [`UiInbox`] has been dropped. Senders should stop.
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    fn push(&self, key: Option<CoalesceKey>, msg: M) -> Result<(), SendError<M>> {
        let shared = &*self.shared;
        let mut q = shared.lock();
        // Checked under the lock: `UiInbox::drop` closes under the same lock,
        // so a message is either delivered to a live inbox or rejected.
        if q.closed {
            drop(q);
            return Err(SendError::Disconnected(msg));
        }
        if let Some(k) = key
            && let Some(&seq) = q.keyed.get(&k)
        {
            let idx = (seq - q.front_seq) as usize;
            let old = std::mem::replace(&mut q.slots[idx].msg, msg);
            drop(q);
            drop(old);
            return Ok(());
        }
        if q.slots.len() >= shared.capacity {
            drop(q);
            return Err(SendError::Full(msg));
        }
        let was_empty = q.slots.is_empty();
        q.push_back(key, msg);
        drop(q);
        if was_empty {
            shared.fire_wake();
        }
        Ok(())
    }
}

/// How much one [`UiInbox::drain`] call may do. At least one message is always
/// delivered if any is queued and `max_messages > 0`, so a frame that is already
/// over budget still makes progress and a burst cannot starve the UI of itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrainBudget {
    pub max_messages: usize,
    pub max_time: Duration,
}

impl DrainBudget {
    pub const UNLIMITED: DrainBudget = DrainBudget {
        max_messages: usize::MAX,
        max_time: Duration::MAX,
    };

    pub fn time(max_time: Duration) -> Self {
        Self {
            max_messages: usize::MAX,
            max_time,
        }
    }

    pub fn count(max_messages: usize) -> Self {
        Self {
            max_messages,
            max_time: Duration::MAX,
        }
    }
}

/// What one drain did; feeds `FrameReport` via [`crate::FrameReport::record_drain`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrainReport {
    pub drained: usize,
    /// Messages still queued. Non-zero means the UI needs another frame.
    pub remaining: usize,
    pub elapsed: Duration,
}

/// Receiving side, owned by the UI thread. Dropping it disconnects every
/// handle and discards queued messages.
pub struct UiInbox<M> {
    shared: Arc<Shared<M>>,
}

impl<M> fmt::Debug for UiInbox<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiInbox")
            .field("capacity", &self.shared.capacity)
            .field("len", &self.len())
            .finish()
    }
}

impl<M> UiInbox<M> {
    /// Creates an inbox holding at most `capacity` messages (minimum 1).
    pub fn channel(capacity: usize) -> (UiInbox<M>, UiHandle<M>) {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue {
                slots: VecDeque::new(),
                front_seq: 0,
                keyed: HashMap::new(),
                closed: false,
            }),
            capacity: capacity.max(1),
            closed: AtomicBool::new(false),
            wake: OnceLock::new(),
        });
        (
            UiInbox {
                shared: Arc::clone(&shared),
            },
            UiHandle { shared },
        )
    }

    /// Like [`UiInbox::channel`] with the wake hook installed up front.
    pub fn channel_with_wake(capacity: usize, wake: WakeHook) -> (UiInbox<M>, UiHandle<M>) {
        let (inbox, handle) = Self::channel(capacity);
        let _ = inbox.shared.wake.set(wake);
        (inbox, handle)
    }

    /// Installs the wake hook once, for shells that only get their repaint
    /// handle after the inbox exists. Returns the hook back if one is already
    /// set. If messages are already queued the hook fires immediately, so a
    /// message sent before installation is not stranded.
    pub fn set_wake(&self, wake: WakeHook) -> Result<(), WakeHook> {
        self.shared.wake.set(wake)?;
        if !self.is_empty() {
            self.shared.fire_wake();
        }
        Ok(())
    }

    /// Another sending handle.
    pub fn handle(&self) -> UiHandle<M> {
        UiHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn len(&self) -> usize {
        self.shared.lock().slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }

    /// Delivers queued messages to `handle` in FIFO order until the queue is
    /// empty or `budget` is spent. Overflow stays queued (which is what pushes
    /// back on senders). The lock is released while `handle` runs, so a handler
    /// may itself send to this inbox.
    pub fn drain(
        &self,
        budget: DrainBudget,
        clock: &dyn Clock,
        mut handle: impl FnMut(M),
    ) -> DrainReport {
        let start = clock.now();
        let mut drained = 0usize;
        let remaining;
        loop {
            if drained >= budget.max_messages
                || (drained > 0 && clock.now() - start >= budget.max_time)
            {
                remaining = self.len();
                break;
            }
            let next = self.shared.lock().pop_front();
            match next {
                Some(m) => {
                    handle(m);
                    drained += 1;
                }
                None => {
                    remaining = 0;
                    break;
                }
            }
        }
        if remaining > 0 {
            // Messages left behind: no sender will transition empty -> non-empty
            // for them, so ask for another frame ourselves.
            self.shared.fire_wake();
        }
        DrainReport {
            drained,
            remaining,
            elapsed: clock.now() - start,
        }
    }
}

impl<M> Drop for UiInbox<M> {
    fn drop(&mut self) {
        let (slots, keyed) = {
            let mut q = self.shared.lock();
            q.closed = true;
            self.shared.closed.store(true, Ordering::Release);
            (std::mem::take(&mut q.slots), std::mem::take(&mut q.keyed))
        };
        // Dropped outside the lock: arbitrary `M::drop` code.
        drop(slots);
        drop(keyed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::VirtualClock;
    use std::sync::atomic::AtomicUsize;

    fn counting_wake() -> (WakeHook, Arc<AtomicUsize>) {
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = Arc::clone(&n);
        (
            Arc::new(move || {
                n2.fetch_add(1, Ordering::SeqCst);
            }),
            n,
        )
    }

    fn drain_all(inbox: &UiInbox<u32>) -> Vec<u32> {
        let mut out = Vec::new();
        inbox.drain(DrainBudget::UNLIMITED, &VirtualClock::new(), |m| {
            out.push(m)
        });
        out
    }

    #[test]
    fn fifo_and_bounded() {
        let (inbox, tx) = UiInbox::channel(3);
        for i in 0..3 {
            tx.send(i).unwrap();
        }
        let err = tx.send(99).unwrap_err();
        assert!(err.is_full());
        assert_eq!(err.into_inner(), 99);
        assert_eq!(drain_all(&inbox), vec![0, 1, 2]);
        tx.send(7).unwrap();
        assert_eq!(drain_all(&inbox), vec![7]);
    }

    #[test]
    fn coalescing_keeps_latest_per_key_in_one_slot() {
        let (inbox, tx) = UiInbox::channel(2);
        let (a, b) = (CoalesceKey(1), CoalesceKey(2));
        for i in 0..100 {
            tx.send_coalesced(a, i).unwrap();
        }
        tx.send_coalesced(b, 500).unwrap();
        tx.send_coalesced(b, 501).unwrap();
        // Both slots in use, but replacement never reports Full.
        tx.send_coalesced(a, 1000).unwrap();
        assert!(tx.send(1).unwrap_err().is_full());
        assert_eq!(inbox.len(), 2);
        assert_eq!(drain_all(&inbox), vec![1000, 501]);
        // After the drain the key is free again.
        tx.send_coalesced(a, 5).unwrap();
        assert_eq!(drain_all(&inbox), vec![5]);
    }

    #[test]
    fn coalescing_index_survives_pops() {
        let (inbox, tx) = UiInbox::channel(8);
        tx.send(0).unwrap();
        tx.send(1).unwrap();
        tx.send_coalesced(CoalesceKey(9), 2).unwrap();
        let clock = VirtualClock::new();
        let mut first = Vec::new();
        inbox.drain(DrainBudget::count(1), &clock, |m| first.push(m));
        assert_eq!(first, vec![0]);
        tx.send_coalesced(CoalesceKey(9), 3).unwrap();
        assert_eq!(drain_all(&inbox), vec![1, 3]);
    }

    #[test]
    fn wake_fires_once_per_empty_to_non_empty_transition() {
        let (wake, count) = counting_wake();
        let (inbox, tx) = UiInbox::channel_with_wake(16, wake);
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send_coalesced(CoalesceKey(1), 3).unwrap();
        tx.send_coalesced(CoalesceKey(1), 4).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(drain_all(&inbox), vec![1, 2, 4]);
        tx.send(5).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
        // A rejected send does not wake.
        let (wake, count) = counting_wake();
        let (_inbox, tx) = UiInbox::channel_with_wake(1, wake);
        tx.send(1).unwrap();
        let _ = tx.send(2);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn late_wake_hook_fires_for_stranded_messages() {
        let (inbox, tx) = UiInbox::channel(4);
        tx.send(1).unwrap();
        let (wake, count) = counting_wake();
        assert!(inbox.set_wake(Arc::clone(&wake)).is_ok());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(inbox.set_wake(wake).is_err());
    }

    #[test]
    fn drain_respects_count_budget_and_leaves_overflow() {
        let (wake, count) = counting_wake();
        let (inbox, tx) = UiInbox::channel_with_wake(16, wake);
        for i in 0..10 {
            tx.send(i).unwrap();
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let clock = VirtualClock::new();
        let mut got = Vec::new();
        let r = inbox.drain(DrainBudget::count(4), &clock, |m| got.push(m));
        assert_eq!((r.drained, r.remaining), (4, 6));
        assert_eq!(got, vec![0, 1, 2, 3]);
        // Leftovers re-arm the wake so the shell schedules another frame.
        assert_eq!(count.load(Ordering::SeqCst), 2);
        let r = inbox.drain(DrainBudget::UNLIMITED, &clock, |_| {});
        assert_eq!((r.drained, r.remaining), (6, 0));
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn drain_respects_time_budget_and_reports_elapsed() {
        let (inbox, tx) = UiInbox::channel(16);
        for i in 0..10 {
            tx.send(i).unwrap();
        }
        let clock = VirtualClock::new();
        // Each handled message costs 1 ms of virtual time.
        let r = inbox.drain(DrainBudget::time(Duration::from_millis(3)), &clock, |_| {
            clock.advance_ms(1)
        });
        assert_eq!(r.drained, 3);
        assert_eq!(r.remaining, 7);
        assert_eq!(r.elapsed, Duration::from_millis(3));
    }

    #[test]
    fn zero_time_budget_still_makes_progress() {
        let (inbox, tx) = UiInbox::channel(4);
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        let r = inbox.drain(
            DrainBudget::time(Duration::ZERO),
            &VirtualClock::new(),
            |_| {},
        );
        assert_eq!((r.drained, r.remaining), (1, 1));
    }

    #[test]
    fn handler_may_send_reentrantly() {
        let (inbox, tx) = UiInbox::channel(4);
        tx.send(1).unwrap();
        let tx2 = tx.clone();
        let mut seen = Vec::new();
        inbox.drain(DrainBudget::UNLIMITED, &VirtualClock::new(), |m| {
            seen.push(m);
            if m == 1 {
                tx2.send(2).unwrap();
            }
        });
        assert_eq!(seen, vec![1, 2]);
    }

    #[test]
    fn dropping_inbox_disconnects_handles_and_drops_queued_messages() {
        let payload = Arc::new(());
        let (inbox, tx) = UiInbox::channel(4);
        tx.send(Arc::clone(&payload)).unwrap();
        assert_eq!(Arc::strong_count(&payload), 2);
        assert!(!tx.is_closed());
        drop(inbox);
        assert!(tx.is_closed());
        assert_eq!(Arc::strong_count(&payload), 1);
        let err = tx.send(Arc::clone(&payload)).unwrap_err();
        assert!(err.is_disconnected());
        assert!(
            tx.send_coalesced(CoalesceKey(1), payload)
                .unwrap_err()
                .is_disconnected()
        );
    }

    #[test]
    fn dropping_handles_does_not_disconnect_inbox() {
        let (inbox, tx) = UiInbox::<u32>::channel(4);
        drop(tx);
        assert!(inbox.is_empty());
        let tx = inbox.handle();
        tx.send(1).unwrap();
        assert_eq!(drain_all(&inbox), vec![1]);
    }

    #[test]
    fn many_std_threads_lose_nothing_and_wake_is_bounded() {
        const THREADS: usize = 8;
        const PER: usize = 500;
        let (wake, wakes) = counting_wake();
        let (inbox, tx) = UiInbox::channel_with_wake(THREADS * PER, wake);
        let joins: Vec<_> = (0..THREADS)
            .map(|t| {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for i in 0..PER {
                        tx.send((t * PER + i) as u32).unwrap();
                    }
                })
            })
            .collect();
        for j in joins {
            j.join().unwrap();
        }
        let mut got = drain_all(&inbox);
        got.sort_unstable();
        assert_eq!(got, (0..(THREADS * PER) as u32).collect::<Vec<_>>());
        // Nothing drained concurrently, so exactly one transition happened.
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_send_and_drain_delivers_everything_in_per_sender_order() {
        const THREADS: u32 = 4;
        const PER: u32 = 2000;
        let (inbox, tx) = UiInbox::<(u32, u32)>::channel(64);
        let joins: Vec<_> = (0..THREADS)
            .map(|t| {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for i in 0..PER {
                        let mut m = (t, i);
                        // Bounded queue: back off until the UI drains.
                        loop {
                            match tx.send(m) {
                                Ok(()) => break,
                                Err(SendError::Full(back)) => {
                                    m = back;
                                    std::thread::yield_now();
                                }
                                Err(e) => panic!("{e}"),
                            }
                        }
                    }
                })
            })
            .collect();
        let clock = VirtualClock::new();
        let mut last = [None::<u32>; THREADS as usize];
        let mut total = 0u32;
        while total < THREADS * PER {
            inbox.drain(DrainBudget::count(32), &clock, |(t, i)| {
                let slot = &mut last[t as usize];
                assert!(slot.is_none_or(|p| p + 1 == i), "reordered or lost");
                *slot = Some(i);
                total += 1;
            });
            std::thread::yield_now();
        }
        for j in joins {
            j.join().unwrap();
        }
    }

    #[test]
    fn coalesced_flood_from_threads_never_fills_inbox() {
        let (inbox, tx) = UiInbox::<u32>::channel(1);
        let joins: Vec<_> = (0..4)
            .map(|_| {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for i in 0..1000 {
                        tx.send_coalesced(CoalesceKey(7), i).unwrap();
                    }
                })
            })
            .collect();
        for j in joins {
            j.join().unwrap();
        }
        assert_eq!(inbox.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn senders_work_from_tokio_tasks() {
        let (inbox, tx) = UiInbox::<u32>::channel(100);
        let tasks: Vec<_> = (0..4)
            .map(|t| {
                let tx = tx.clone();
                tokio::spawn(async move {
                    for i in 0..10 {
                        tx.send(t * 10 + i).unwrap();
                        tokio::task::yield_now().await;
                    }
                })
            })
            .collect();
        for t in tasks {
            t.await.unwrap();
        }
        assert_eq!(drain_all(&inbox).len(), 40);
    }
}
