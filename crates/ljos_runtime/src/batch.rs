//! Bounded side streams into the UI thread (ARCHITECTURE.md 3.3).
//!
//! A thread that feeds the UI many small items (folder events, a child
//! process's output, chunks of a transcript, the deltas of a stream) should
//! neither post one message per item nor wait for room: the first floods
//! the inbox and runs a turn per item, the second ties the producer to the
//! frame rate. A side stream is a second, typed queue next to the
//! [`UiInbox`](crate::UiInbox), made from its [`UiHandle`](crate::UiHandle)
//! with [`UiHandle::batch`](crate::UiHandle::batch) and woken through the
//! same [`WakeHook`]. The UI thread takes everything queued at once and
//! hands it to one handler as a [`Drain`], so a burst is one turn.
//!
//! # Bounded, never blocking
//!
//! A stream holds at most its capacity, fixed when it is made, in a ring
//! allocated up front. [`BatchSender::send`] never waits: when the stream
//! is full, its [`BatchPolicy`] says what gives way, and an observer counts
//! it (`replaced`, `refused` or `evicted`).
//!
//! # Wake protocol
//!
//! As the inbox's: the hook fires once per transition from empty to
//! non-empty, decided under the lock, so a burst of ten thousand items
//! wakes the UI thread once. A flag beside the lock tells the UI thread
//! whether there is anything to take, so a frame that finds every stream
//! empty takes no lock. A drain that leaves items behind fires the hook
//! again, so a partly drained stream always schedules another frame.
//!
//! # Moving items out without allocating
//!
//! The UI side keeps a second ring of the same capacity. A drain swaps the
//! two under the lock (or moves the front of the queue across, when the
//! budget takes only part of it), then runs the handler outside the lock,
//! so a sender is never held up by the handler and the drain allocates
//! nothing.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Instant;

use crate::inbox::{CoalesceKey, DrainBudget, DrainReport, SendError, WakeHook};
use crate::observe::{self, MailboxKind, MailboxProbe, MailboxSample};
use crate::time::Clock;

/// What gives way when a stream is full, chosen when it is made.
pub enum BatchPolicy<T> {
    /// A newer item replaces the queued one with the same key, in its
    /// place, counted `replaced`. An item of a key not queued needs a free
    /// slot; on a full stream it is refused, counted `refused`. For
    /// latest-value items: a progress, a file's state, a model row.
    Coalesce(fn(&T) -> CoalesceKey),
    /// A full stream refuses the new item and hands it back, counted
    /// `refused`. For a stream whose oldest items matter most: the start
    /// of a log.
    DropNewest,
    /// A full stream evicts its oldest item to make room for the new one,
    /// counted `evicted`. For a stream whose newest items matter most: the
    /// tail of a child process's output, recent events.
    DropOldest,
}

impl<T> Clone for BatchPolicy<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for BatchPolicy<T> {}

// Manual so `T` need not be `Debug`.
impl<T> fmt::Debug for BatchPolicy<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Coalesce(_) => "Coalesce(..)",
            Self::DropNewest => "DropNewest",
            Self::DropOldest => "DropOldest",
        })
    }
}

struct Item<T> {
    key: Option<CoalesceKey>,
    value: T,
    /// When it was queued, kept only while observed.
    at: Option<Instant>,
}

/// FIFO with O(1) in-place replacement by key, as the inbox's queue: an
/// item's sequence number minus `front_seq` is its index.
struct Ring<T> {
    items: VecDeque<Item<T>>,
    front_seq: u64,
    keyed: HashMap<CoalesceKey, u64>,
    closed: bool,
}

impl<T> Ring<T> {
    fn push_back(&mut self, key: Option<CoalesceKey>, value: T) {
        if let Some(k) = key {
            self.keyed
                .insert(k, self.front_seq + self.items.len() as u64);
        }
        let at = observe::is_enabled().then(Instant::now);
        self.items.push_back(Item { key, value, at });
    }

    fn pop_front(&mut self) -> Option<T> {
        let item = self.items.pop_front()?;
        self.front_seq += 1;
        if let Some(k) = item.key {
            self.keyed.remove(&k);
        }
        Some(item.value)
    }
}

/// Moved only while observed (see [`crate::observe`]).
#[derive(Default)]
struct Counts {
    sent: AtomicU64,
    refused: AtomicU64,
    replaced: AtomicU64,
    evicted: AtomicU64,
    drained: AtomicU64,
}

fn bump(counter: &AtomicU64, by: u64) {
    if observe::is_enabled() {
        counter.fetch_add(by, Ordering::Relaxed);
    }
}

struct Shared<T> {
    name: &'static str,
    ring: Mutex<Ring<T>>,
    capacity: usize,
    policy: BatchPolicy<T>,
    /// Whether the ring holds anything, set and cleared under its lock:
    /// the UI thread reads it without the lock to skip an empty stream.
    pending: AtomicBool,
    /// Mirrors `Ring::closed` for lock-free `is_closed`.
    closed: AtomicBool,
    wake: WakeHook,
    counts: Counts,
}

impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, Ring<T>> {
        // As the inbox: every mutation leaves the ring consistent, and a
        // sender's panic must not take the UI down.
        self.ring.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// What a send did, decided under the lock and finished outside it.
enum Sent<T> {
    Queued {
        was_empty: bool,
    },
    /// The item queued under the same key, to drop outside the lock.
    Replaced(T),
    /// The oldest item, to drop outside the lock.
    Evicted(T),
    /// The item itself, handed back.
    Refused(T),
}

/// Makes a stream that wakes the UI through `wake`. Public through
/// [`UiHandle::batch`](crate::UiHandle::batch) only, so every stream is
/// bound to an inbox's wake (rule 7: threads reach the Engine through its
/// handle).
pub(crate) fn channel<T: Send + 'static>(
    name: &'static str,
    capacity: usize,
    policy: BatchPolicy<T>,
    wake: WakeHook,
) -> (UiBatch<T>, BatchSender<T>) {
    let capacity = capacity.max(1);
    let keyed = match policy {
        BatchPolicy::Coalesce(_) => HashMap::with_capacity(capacity),
        BatchPolicy::DropNewest | BatchPolicy::DropOldest => HashMap::new(),
    };
    let shared = Arc::new(Shared {
        name,
        ring: Mutex::new(Ring {
            items: VecDeque::with_capacity(capacity),
            front_seq: 0,
            keyed,
            closed: false,
        }),
        capacity,
        policy,
        pending: AtomicBool::new(false),
        closed: AtomicBool::new(false),
        wake,
        counts: Counts::default(),
    });
    let weak: Weak<Shared<T>> = Arc::downgrade(&shared);
    observe::register(weak);
    (
        UiBatch {
            shared: Arc::clone(&shared),
            spare: VecDeque::with_capacity(capacity),
        },
        BatchSender { shared },
    )
}

/// The sending side of a side stream. `Clone + Send + Sync`; works from
/// std threads and async tasks alike, and never blocks.
pub struct BatchSender<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Clone for BatchSender<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> fmt::Debug for BatchSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchSender")
            .field("name", &self.shared.name)
            .field("capacity", &self.shared.capacity)
            .field("policy", &self.shared.policy)
            .field("closed", &self.is_closed())
            .finish()
    }
}

impl<T> BatchSender<T> {
    /// Queues `item`, or does what the stream's [`BatchPolicy`] says when
    /// it is full. Never waits. Only [`BatchPolicy::DropNewest`], and
    /// [`BatchPolicy::Coalesce`] for a key not queued, refuse an item:
    /// [`SendError::Full`] hands it back. [`SendError::Disconnected`] once
    /// the [`UiBatch`] is gone.
    pub fn send(&self, item: T) -> Result<(), SendError<T>> {
        let shared = &*self.shared;
        let mut ring = shared.lock();
        // Checked under the lock `UiBatch::drop` closes under: an item is
        // either queued in a live stream or handed back.
        if ring.closed {
            return Err(SendError::Disconnected(item));
        }
        let full = ring.items.len() >= shared.capacity;
        let sent = match shared.policy {
            BatchPolicy::Coalesce(key_of) => {
                let key = key_of(&item);
                if let Some(&seq) = ring.keyed.get(&key) {
                    let idx = (seq - ring.front_seq) as usize;
                    Sent::Replaced(std::mem::replace(&mut ring.items[idx].value, item))
                } else if full {
                    Sent::Refused(item)
                } else {
                    let was_empty = ring.items.is_empty();
                    ring.push_back(Some(key), item);
                    Sent::Queued { was_empty }
                }
            }
            BatchPolicy::DropNewest if full => Sent::Refused(item),
            BatchPolicy::DropOldest if full => {
                let old = ring.pop_front().expect("a full ring has a front");
                ring.push_back(None, item);
                Sent::Evicted(old)
            }
            BatchPolicy::DropNewest | BatchPolicy::DropOldest => {
                let was_empty = ring.items.is_empty();
                ring.push_back(None, item);
                Sent::Queued { was_empty }
            }
        };
        if matches!(sent, Sent::Queued { was_empty: true }) {
            shared.pending.store(true, Ordering::Release);
        }
        drop(ring);
        let counts = &shared.counts;
        match sent {
            Sent::Refused(item) => {
                bump(&counts.refused, 1);
                return Err(SendError::Full(item));
            }
            Sent::Queued { was_empty } => {
                bump(&counts.sent, 1);
                if was_empty {
                    (shared.wake)();
                }
            }
            // Dropped outside the lock: arbitrary `T::drop` code.
            Sent::Replaced(old) => {
                bump(&counts.sent, 1);
                bump(&counts.replaced, 1);
                drop(old);
            }
            Sent::Evicted(old) => {
                bump(&counts.sent, 1);
                bump(&counts.evicted, 1);
                drop(old);
            }
        }
        Ok(())
    }

    /// True once the [`UiBatch`] has been dropped. Senders should stop.
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    /// The name the stream is observed by.
    pub fn name(&self) -> &'static str {
        self.shared.name
    }

    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }
}

/// The receiving side of a side stream, owned by the UI thread. The shell
/// drains it after the application's inbox, one turn per stream per frame.
pub struct UiBatch<T> {
    shared: Arc<Shared<T>>,
    /// The second ring a drain swaps in, so the handler runs outside the
    /// lock and nothing is allocated. Empty between drains.
    spare: VecDeque<Item<T>>,
}

impl<T> fmt::Debug for UiBatch<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiBatch")
            .field("name", &self.shared.name)
            .field("capacity", &self.shared.capacity)
            .field("policy", &self.shared.policy)
            .field("len", &self.len())
            .finish()
    }
}

impl<T> UiBatch<T> {
    /// The name it was made with, which an observer lists it by and a
    /// turn that runs long is named after.
    pub fn name(&self) -> &'static str {
        self.shared.name
    }

    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }

    pub fn policy(&self) -> BatchPolicy<T> {
        self.shared.policy
    }

    /// Items waiting now.
    pub fn len(&self) -> usize {
        self.shared.lock().items.len()
    }

    pub fn is_empty(&self) -> bool {
        !self.has_pending()
    }

    /// Whether anything is queued, without taking the lock.
    pub fn has_pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }

    /// Another sender of this stream.
    pub fn sender(&self) -> BatchSender<T> {
        BatchSender {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Whether `sender` sends into this stream.
    pub fn is_fed_by(&self, sender: &BatchSender<T>) -> bool {
        Arc::ptr_eq(&self.shared, &sender.shared)
    }

    /// Takes what is queued, at most `budget.max_messages` items and at
    /// least one, and hands it to `handle` in the order it was sent, as
    /// one run. Nothing is taken and `handle` is not called when the
    /// stream is empty. Items `handle` leaves in the [`Drain`] are dropped.
    ///
    /// The time budget does not split a run: the handler gets the items
    /// together or not at all, and its run is bounded by the stream's
    /// capacity and the count. Items left behind fire the wake hook again,
    /// so a partly drained stream schedules another frame.
    pub fn drain(
        &mut self,
        budget: DrainBudget,
        clock: &dyn Clock,
        handle: impl FnOnce(Drain<'_, T>),
    ) -> DrainReport {
        if !self.has_pending() {
            return DrainReport::default();
        }
        // A handler that unwound out of the last drain left its items
        // here: they were that run's, and must not be swapped back into
        // the queue to be handed over again.
        self.spare.clear();
        let take = budget.max_messages.max(1);
        let (drained, remaining) = {
            let mut guard = self.shared.lock();
            let ring = &mut *guard;
            let n = ring.items.len();
            if n <= take {
                std::mem::swap(&mut ring.items, &mut self.spare);
                ring.keyed.clear();
            } else {
                for item in ring.items.drain(..take) {
                    if let Some(k) = item.key {
                        ring.keyed.remove(&k);
                    }
                    self.spare.push_back(item);
                }
            }
            let drained = n.min(take);
            ring.front_seq += drained as u64;
            let remaining = ring.items.len();
            if remaining == 0 {
                self.shared.pending.store(false, Ordering::Release);
            }
            (drained, remaining)
        };
        if drained == 0 {
            return DrainReport::default();
        }
        bump(&self.shared.counts.drained, drained as u64);
        if remaining > 0 {
            // Before the handler: no sender will move the stream from
            // empty to non-empty for what is left, and a handler that
            // panics must not leave it unscheduled.
            (self.shared.wake)();
        }
        let start = clock.now();
        handle(Drain {
            items: &mut self.spare,
        });
        // Whatever the handler left, dropped here, before the next drain
        // swaps this ring back in.
        self.spare.clear();
        DrainReport {
            drained,
            remaining,
            elapsed: clock.now() - start,
        }
    }
}

impl<T> Drop for UiBatch<T> {
    fn drop(&mut self) {
        let (items, keyed) = {
            let mut ring = self.shared.lock();
            ring.closed = true;
            self.shared.closed.store(true, Ordering::Release);
            self.shared.pending.store(false, Ordering::Release);
            (
                std::mem::take(&mut ring.items),
                std::mem::take(&mut ring.keyed),
            )
        };
        // Dropped outside the lock: arbitrary `T::drop` code.
        drop(items);
        drop(keyed);
    }
}

/// One run of a stream's items, front first, handed to its handler.
/// Items it is not asked for are dropped with it.
pub struct Drain<'a, T> {
    items: &'a mut VecDeque<Item<T>>,
}

impl<T> fmt::Debug for Drain<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Drain")
            .field("len", &self.items.len())
            .finish()
    }
}

impl<T> Iterator for Drain<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.items.pop_front().map(|item| item.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.items.len();
        (n, Some(n))
    }
}

impl<T> ExactSizeIterator for Drain<'_, T> {}

impl<T> DoubleEndedIterator for Drain<'_, T> {
    fn next_back(&mut self) -> Option<T> {
        self.items.pop_back().map(|item| item.value)
    }
}

impl<T> std::iter::FusedIterator for Drain<'_, T> {}

impl<T: Send> MailboxProbe for Shared<T> {
    fn sample(self: Arc<Self>, _peek: usize) -> Option<MailboxSample> {
        if self.closed.load(Ordering::Acquire) {
            return None;
        }
        let (depth, oldest) = {
            let ring = self.lock();
            let oldest = ring
                .items
                .front()
                .and_then(|item| item.at)
                .map(|at| at.elapsed());
            (ring.items.len(), oldest)
        };
        let c = &self.counts;
        Some(MailboxSample {
            name: self.name,
            kind: MailboxKind::Batch,
            capacity: self.capacity,
            depth,
            oldest,
            sent: c.sent.load(Ordering::Relaxed),
            refused: c.refused.load(Ordering::Relaxed),
            replaced: c.replaced.load(Ordering::Relaxed),
            evicted: c.evicted.load(Ordering::Relaxed),
            drained: c.drained.load(Ordering::Relaxed),
            // Besides the senders, the stream and this sample hold one.
            senders: Arc::strong_count(&self).saturating_sub(2),
            // A side stream's senders never wait, and its items have no
            // kinds: a stream carries one type.
            waiting: 0,
            kinds: Vec::new(),
            peek: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox::UiInbox;
    use crate::time::VirtualClock;
    use std::sync::atomic::AtomicUsize;

    /// A stream of `cap` made from an inbox whose wake counts.
    fn stream<T: Send + 'static>(
        name: &'static str,
        cap: usize,
        policy: BatchPolicy<T>,
    ) -> (UiInbox<()>, UiBatch<T>, BatchSender<T>, Arc<AtomicUsize>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&wakes);
        let (inbox, handle) = UiInbox::<()>::channel_with_wake(
            4,
            Arc::new(move || {
                counted.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let (batch, tx) = handle.batch(name, cap, policy);
        (inbox, batch, tx, wakes)
    }

    fn all<T>(batch: &mut UiBatch<T>) -> (Vec<T>, DrainReport) {
        let mut got = Vec::new();
        let report = batch.drain(DrainBudget::UNLIMITED, &VirtualClock::new(), |d| {
            got.extend(d);
        });
        (got, report)
    }

    fn sample_of(name: &str) -> Option<MailboxSample> {
        observe::mailboxes(4).into_iter().find(|m| m.name == name)
    }

    #[test]
    fn a_burst_wakes_once_and_is_one_run() {
        let (_inbox, mut batch, tx, wakes) = stream("test.burst", 10_000, BatchPolicy::DropNewest);
        for i in 0..10_000u32 {
            tx.send(i).unwrap();
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        let mut runs = 0;
        let mut items = 0;
        let report = batch.drain(DrainBudget::UNLIMITED, &VirtualClock::new(), |d| {
            runs += 1;
            assert_eq!(d.len(), 10_000);
            items += d.count();
        });
        assert_eq!((runs, items), (1, 10_000));
        assert_eq!((report.drained, report.remaining), (10_000, 0));
        assert!(batch.is_empty());
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_empty_stream_runs_no_handler_and_wakes_nothing() {
        let (_inbox, mut batch, _tx, wakes) = stream::<u8>("test.idle", 4, BatchPolicy::DropNewest);
        let mut called = false;
        let report = batch.drain(DrainBudget::UNLIMITED, &VirtualClock::new(), |_| {
            called = true
        });
        assert!(!called);
        assert_eq!(report, DrainReport::default());
        assert_eq!(wakes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn coalescing_keeps_the_newest_per_key_in_the_first_place() {
        fn key(item: &(char, u32)) -> CoalesceKey {
            CoalesceKey(item.0 as u64)
        }
        let (_inbox, mut batch, tx, _) = stream("test.coalesce", 2, BatchPolicy::Coalesce(key));
        for item in [('a', 1), ('b', 1), ('a', 2), ('a', 3), ('b', 2)] {
            tx.send(item).unwrap();
        }
        // A third key needs a free slot.
        assert!(tx.send(('c', 1)).unwrap_err().is_full());
        assert_eq!(all(&mut batch).0, vec![('a', 3), ('b', 2)]);
        // The keys went with the run: a key sent again is queued anew.
        tx.send(('a', 4)).unwrap();
        tx.send(('c', 1)).unwrap();
        assert_eq!(all(&mut batch).0, vec![('a', 4), ('c', 1)]);
    }

    #[test]
    fn a_partial_run_takes_the_keys_of_what_it_took() {
        fn key(item: &(char, u32)) -> CoalesceKey {
            CoalesceKey(item.0 as u64)
        }
        let (_inbox, mut batch, tx, _) = stream("test.partial", 4, BatchPolicy::Coalesce(key));
        for item in [('a', 1), ('b', 1), ('c', 1)] {
            tx.send(item).unwrap();
        }
        let mut got = Vec::new();
        batch.drain(DrainBudget::count(2), &VirtualClock::new(), |d| {
            got.extend(d)
        });
        assert_eq!(got, vec![('a', 1), ('b', 1)]);
        // `c` is still queued and is replaced in place; `a` is queued anew.
        tx.send(('a', 2)).unwrap();
        tx.send(('c', 2)).unwrap();
        assert_eq!(all(&mut batch).0, vec![('c', 2), ('a', 2)]);
    }

    #[test]
    fn drop_newest_refuses_and_hands_the_item_back() {
        let (_inbox, mut batch, tx, _) = stream("test.newest", 2, BatchPolicy::DropNewest);
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        assert_eq!(tx.send(3).unwrap_err().into_inner(), 3);
        assert_eq!(all(&mut batch).0, vec![1, 2]);
    }

    #[test]
    fn drop_oldest_evicts_to_make_room() {
        let (_inbox, mut batch, tx, wakes) = stream("test.oldest", 3, BatchPolicy::DropOldest);
        for i in 1..=5 {
            tx.send(i).unwrap();
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert_eq!(all(&mut batch).0, vec![3, 4, 5]);
    }

    #[test]
    fn a_run_cut_short_by_the_budget_wakes_again() {
        let (_inbox, mut batch, tx, wakes) = stream("test.budget", 16, BatchPolicy::DropNewest);
        for i in 0..10 {
            tx.send(i).unwrap();
        }
        let mut got = Vec::new();
        let report = batch.drain(DrainBudget::count(4), &VirtualClock::new(), |d| {
            got.extend(d)
        });
        assert_eq!(got, vec![0, 1, 2, 3]);
        assert_eq!((report.drained, report.remaining), (4, 6));
        assert!(batch.has_pending());
        assert_eq!(wakes.load(Ordering::SeqCst), 2);
        // A budget of no messages still takes one, as the inbox does.
        let report = batch.drain(DrainBudget::count(0), &VirtualClock::new(), |d| {
            got.extend(d)
        });
        assert_eq!(report.drained, 1);
        let (rest, report) = all(&mut batch);
        assert_eq!(rest, vec![5, 6, 7, 8, 9]);
        assert_eq!(report.remaining, 0);
        assert!(!batch.has_pending());
        assert_eq!(wakes.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn items_the_handler_leaves_are_dropped_with_the_run() {
        let dropped = Arc::new(AtomicUsize::new(0));
        struct Counted(Arc<AtomicUsize>);
        impl Drop for Counted {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let (_inbox, mut batch, tx, _) = stream("test.leave", 4, BatchPolicy::DropNewest);
        for _ in 0..3 {
            tx.send(Counted(Arc::clone(&dropped))).unwrap();
        }
        batch.drain(DrainBudget::UNLIMITED, &VirtualClock::new(), |mut d| {
            drop(d.next());
        });
        assert_eq!(dropped.load(Ordering::SeqCst), 3);
        assert!(batch.is_empty());
    }

    #[test]
    fn a_run_whose_handler_unwinds_is_never_handed_over_again() {
        let (_inbox, mut batch, tx, _) = stream("test.unwind", 8, BatchPolicy::DropNewest);
        let mut panicking_drain = |batch: &mut UiBatch<u32>, budget| {
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                batch.drain(budget, &VirtualClock::new(), |mut d| {
                    d.next();
                    panic!("the handler fails");
                })
            }));
            assert!(caught.is_err());
        };
        // The whole queue taken (its ring swapped), then part of it.
        for i in 1..=3 {
            tx.send(i).unwrap();
        }
        panicking_drain(&mut batch, DrainBudget::UNLIMITED);
        tx.send(4).unwrap();
        assert_eq!(all(&mut batch).0, vec![4]);
        assert!(batch.is_empty());
        for i in 5..=9 {
            tx.send(i).unwrap();
        }
        panicking_drain(&mut batch, DrainBudget::count(2));
        assert_eq!(all(&mut batch).0, vec![7, 8, 9]);
        assert!(batch.is_empty());
    }

    #[test]
    fn the_wake_an_inbox_is_given_later_wakes_its_streams() {
        let (inbox, handle) = UiInbox::<()>::channel(4);
        let (mut batch, tx) = handle.batch("test.later", 4, BatchPolicy::DropNewest);
        // No hook yet: queued, and nobody to wake.
        tx.send(1u8).unwrap();
        all(&mut batch);
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&wakes);
        let set = inbox.set_wake(Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }));
        assert!(set.is_ok());
        tx.send(2).unwrap();
        tx.send(3).unwrap();
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert_eq!(all(&mut batch).0, vec![2, 3]);
    }

    #[test]
    fn a_dropped_stream_disconnects_its_senders() {
        let (_inbox, batch, tx, _) = stream("test.closed", 2, BatchPolicy::DropOldest);
        tx.send(1).unwrap();
        drop(batch);
        assert!(tx.is_closed());
        assert!(tx.send(2).unwrap_err().is_disconnected());
        assert!(sample_of("test.closed").is_none());
    }

    #[test]
    fn senders_on_many_threads_wake_once_and_lose_nothing() {
        let (_inbox, mut batch, tx, wakes) = stream("test.threads", 4_000, BatchPolicy::DropNewest);
        let threads: Vec<_> = (0..4u32)
            .map(|t| {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for i in 0..1_000 {
                        tx.send(t * 1_000 + i).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        let (mut got, _) = all(&mut batch);
        got.sort_unstable();
        assert_eq!(got, (0..4_000).collect::<Vec<_>>());
    }

    #[test]
    fn an_observed_stream_is_a_batch_mailbox_with_its_counts() {
        let guard = observe::enable();
        let (_inbox, mut batch, tx, _) = stream("test.observed-batch", 3, BatchPolicy::DropOldest);
        for i in 0..5 {
            tx.send(i).unwrap();
        }
        let s = sample_of("test.observed-batch").unwrap();
        assert_eq!(s.kind, MailboxKind::Batch);
        assert_eq!((s.capacity, s.depth, s.senders, s.waiting), (3, 3, 1, 0));
        assert_eq!(
            (s.sent, s.refused, s.replaced, s.evicted, s.drained),
            (5, 0, 0, 2, 0)
        );
        assert!(s.oldest.is_some());
        assert!(s.kinds.is_empty() && s.peek.is_empty());
        all(&mut batch);
        let s = sample_of("test.observed-batch").unwrap();
        assert_eq!((s.depth, s.drained, s.oldest), (0, 3, None));
        drop(guard);
    }
}
