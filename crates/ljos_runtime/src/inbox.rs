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
//! support in-place coalescing. The UI thread never waits on a condvar or
//! polls: it sleeps in the shell's event loop and the [`WakeHook`] is the
//! only thing that wakes it. A poisoned lock is recovered, because every mutation
//! leaves the queue consistent and a sender's panic must not take the UI down.
//! Replaced or rejected payloads are dropped outside the lock, since `M::drop`
//! is arbitrary code.
//!
//! # Waiting for room
//!
//! A sender that must not lose a message (an ordered event stream, a file
//! watcher, a child process's output) has no right answer to
//! [`SendError::Full`] except to wait. [`UiHandle::send_blocking`] waits on a
//! condvar and [`UiHandle::send_async`] registers a task waker, both under the
//! queue's lock, and [`UiInbox::drain`] signals them as it frees slots. Only
//! senders wait; a drain that finds no one waiting pays one length check per
//! message. A sender that would rather drop keeps [`UiHandle::send`]'s fast
//! failure, and a latest-value stream keeps [`UiHandle::send_coalesced`].
//!
//! # Wake protocol
//!
//! The hook fires once per transition from empty to non-empty, decided under
//! the lock, so it is exact: no lost wake-up and no hook call per message. If a
//! budgeted [`UiInbox::drain`] leaves messages behind it re-arms the hook, so a
//! partially drained inbox always schedules another frame.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::task::{Context, Poll, Waker};
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
    /// Threads in [`UiHandle::send_blocking`] waiting for a free slot.
    blocked: usize,
    /// Tasks in [`UiHandle::send_async`] waiting for a free slot.
    wakers: Vec<Waker>,
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
    /// Signalled when a slot frees up or the inbox closes, for blocked
    /// senders.
    room: Condvar,
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

    /// Pushes `msg` if there is room, under the lock the caller holds.
    /// `Ok(true)` when the queue was empty before, so the UI needs waking.
    fn try_push(
        &self,
        q: &mut Queue<M>,
        key: Option<CoalesceKey>,
        msg: M,
    ) -> Result<Pushed<M>, SendError<M>> {
        // Checked under the lock: `UiInbox::drop` closes under the same lock,
        // so a message is either delivered to a live inbox or rejected.
        if q.closed {
            return Err(SendError::Disconnected(msg));
        }
        if let Some(k) = key
            && let Some(&seq) = q.keyed.get(&k)
        {
            let idx = (seq - q.front_seq) as usize;
            let old = std::mem::replace(&mut q.slots[idx].msg, msg);
            return Ok(Pushed::Replaced(old));
        }
        if q.slots.len() >= self.capacity {
            return Err(SendError::Full(msg));
        }
        let was_empty = q.slots.is_empty();
        q.push_back(key, msg);
        Ok(Pushed::Queued { was_empty })
    }

    /// Takes the front message and tells waiting senders a slot is free.
    fn pop(&self) -> Option<M> {
        let mut q = self.lock();
        let m = q.pop_front();
        if m.is_none() || (q.blocked == 0 && q.wakers.is_empty()) {
            return m;
        }
        if q.blocked > 0 {
            self.room.notify_one();
        }
        // Every task is woken, not one: a woken future may have been
        // dropped, and its wake-up must not be the only one there was.
        let wakers = std::mem::take(&mut q.wakers);
        drop(q);
        for w in wakers {
            w.wake();
        }
        m
    }
}

/// What a successful push did.
enum Pushed<M> {
    Queued {
        was_empty: bool,
    },
    /// The message waiting under the same key, to drop outside the lock.
    Replaced(M),
}

/// Sending side. `Clone + Send + Sync`; works from std threads and async tasks
/// alike. [`UiHandle::send`] and [`UiHandle::send_coalesced`] never block;
/// [`UiHandle::send_blocking`] and [`UiHandle::send_async`] wait for room.
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
    /// it is replaced in place, so a flood of updates occupies one slot: once
    /// the first message of a key is queued, the ones after it never report
    /// `Full`. That first one needs a free slot like any other message, and on
    /// a full inbox it reports `Full`.
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
        let pushed = shared.try_push(&mut q, key, msg);
        drop(q);
        self.pushed(pushed)
    }

    /// The part of a push done outside the lock: wake the UI, drop what
    /// was replaced or rejected.
    fn pushed(&self, pushed: Result<Pushed<M>, SendError<M>>) -> Result<(), SendError<M>> {
        match pushed? {
            Pushed::Queued { was_empty: true } => self.shared.fire_wake(),
            Pushed::Queued { was_empty: false } => {}
            Pushed::Replaced(old) => drop(old),
        }
        Ok(())
    }

    /// Enqueues `msg`, waiting for a free slot when the inbox is full. For a
    /// sender that must not lose messages and has a thread to spare: it sleeps
    /// on a condvar that [`UiInbox::drain`] signals, without spinning. The
    /// only error is [`SendError::Disconnected`], also when the inbox is
    /// dropped while this waits.
    ///
    /// Never call it on the thread that drains the inbox (the UI thread): with
    /// the inbox full it would wait for itself.
    pub fn send_blocking(&self, msg: M) -> Result<(), SendError<M>> {
        let shared = &*self.shared;
        let mut q = shared.lock();
        let mut msg = msg;
        loop {
            match shared.try_push(&mut q, None, msg) {
                Err(SendError::Full(back)) => msg = back,
                other => {
                    drop(q);
                    return self.pushed(other);
                }
            }
            q.blocked += 1;
            q = shared.room.wait(q).unwrap_or_else(|e| e.into_inner());
            q.blocked -= 1;
        }
    }

    /// Enqueues `msg`, waiting without a thread for a free slot when the inbox
    /// is full: the future's waker is woken when [`UiInbox::drain`] frees one.
    /// Works on any executor. The only error is [`SendError::Disconnected`].
    /// Dropping the future before it completes drops `msg` unsent.
    pub fn send_async(&self, msg: M) -> SendAsync<'_, M> {
        SendAsync {
            handle: self,
            msg: Some(msg),
        }
    }
}

/// The future of [`UiHandle::send_async`].
#[must_use = "a future does nothing unless awaited"]
pub struct SendAsync<'a, M> {
    handle: &'a UiHandle<M>,
    msg: Option<M>,
}

// The message is moved out by value, never pinned in place.
impl<M> Unpin for SendAsync<'_, M> {}

impl<M> fmt::Debug for SendAsync<'_, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SendAsync")
            .field("sent", &self.msg.is_none())
            .finish()
    }
}

impl<M> Future for SendAsync<'_, M> {
    type Output = Result<(), SendError<M>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Some(msg) = self.msg.take() else {
            panic!("SendAsync polled after it completed");
        };
        let shared = &*self.handle.shared;
        let mut q = shared.lock();
        match shared.try_push(&mut q, None, msg) {
            Err(SendError::Full(back)) => {
                // Registered under the lock a drain pops under: a slot freed
                // after this check finds the waker.
                if !q.wakers.iter().any(|w| w.will_wake(cx.waker())) {
                    q.wakers.push(cx.waker().clone());
                }
                drop(q);
                self.msg = Some(back);
                Poll::Pending
            }
            other => {
                drop(q);
                Poll::Ready(self.handle.pushed(other))
            }
        }
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
                blocked: 0,
                wakers: Vec::new(),
            }),
            room: Condvar::new(),
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
            let next = self.shared.pop();
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
        let (slots, keyed, wakers) = {
            let mut q = self.shared.lock();
            q.closed = true;
            self.shared.closed.store(true, Ordering::Release);
            (
                std::mem::take(&mut q.slots),
                std::mem::take(&mut q.keyed),
                std::mem::take(&mut q.wakers),
            )
        };
        // Waiting senders learn they are disconnected.
        self.shared.room.notify_all();
        for w in wakers {
            w.wake();
        }
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

    #[test]
    fn a_new_coalesce_key_needs_a_free_slot() {
        let (inbox, tx) = UiInbox::channel(1);
        tx.send(0).unwrap();
        assert!(tx.send_coalesced(CoalesceKey(7), 1).unwrap_err().is_full());
        assert_eq!(drain_all(&inbox), vec![0]);
        tx.send_coalesced(CoalesceKey(7), 2).unwrap();
        tx.send_coalesced(CoalesceKey(7), 3).unwrap();
        assert_eq!(drain_all(&inbox), vec![3]);
    }

    #[test]
    fn a_blocking_sender_waits_for_the_drain_and_loses_nothing() {
        const N: u32 = 200;
        let (wake, _) = counting_wake();
        let (inbox, tx) = UiInbox::channel_with_wake(4, wake);
        let (sent_tx, sent_rx) = std::sync::mpsc::channel();
        let sender = std::thread::spawn(move || {
            for i in 0..N {
                tx.send_blocking(i).unwrap();
                sent_tx.send(i).unwrap();
            }
        });
        // The sender fills the inbox and then waits: nothing more arrives
        // until this thread drains.
        for i in 0..4 {
            assert_eq!(sent_rx.recv().unwrap(), i);
        }
        assert!(
            sent_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "a full inbox holds the sender back"
        );
        let mut got = Vec::new();
        let clock = VirtualClock::new();
        while got.len() < N as usize {
            inbox.drain(DrainBudget::count(3), &clock, |m| got.push(m));
            // Wait for the sender to have had its turn, on a channel.
            if got.len() < N as usize {
                let _ = sent_rx.recv();
            }
        }
        sender.join().unwrap();
        assert_eq!(got, (0..N).collect::<Vec<_>>());
    }

    #[test]
    fn dropping_the_inbox_releases_a_blocked_sender() {
        let (inbox, tx) = UiInbox::channel(1);
        tx.send(0).unwrap();
        let (waiting_tx, waiting_rx) = std::sync::mpsc::channel();
        let sender = std::thread::spawn(move || {
            waiting_tx.send(()).unwrap();
            tx.send_blocking(1)
        });
        waiting_rx.recv().unwrap();
        drop(inbox);
        let err = sender.join().unwrap().unwrap_err();
        assert!(err.is_disconnected());
        assert_eq!(err.into_inner(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_async_sender_waits_for_the_drain_and_loses_nothing() {
        const N: u32 = 100;
        let (inbox, tx) = UiInbox::<u32>::channel(2);
        let (sent_tx, sent_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            for i in 0..N {
                tx.send_async(i).await.unwrap();
                sent_tx.send(i).unwrap();
            }
        });
        let got = tokio::task::spawn_blocking(move || {
            let mut got = Vec::new();
            let clock = VirtualClock::new();
            while got.len() < N as usize {
                inbox.drain(DrainBudget::count(1), &clock, |m| got.push(m));
                if got.len() < N as usize {
                    let _ = sent_rx.recv();
                }
            }
            got
        })
        .await
        .unwrap();
        task.await.unwrap();
        assert_eq!(got, (0..N).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn an_async_send_to_a_dropped_inbox_is_disconnected() {
        let (inbox, tx) = UiInbox::<u32>::channel(1);
        tx.send(0).unwrap();
        let waiting = tokio::spawn(async move { tx.send_async(1).await });
        tokio::task::yield_now().await;
        drop(inbox);
        assert!(waiting.await.unwrap().unwrap_err().is_disconnected());
    }
}
