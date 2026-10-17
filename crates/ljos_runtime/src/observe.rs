//! What an observer may see of the mailboxes of a process (ARCHITECTURE.md
//! 3.4 and 19.4): every [`UiInbox`](crate::UiInbox) that was named with
//! [`UiInbox::observe_as`](crate::UiInbox::observe_as), its depth, its oldest
//! message's age and what went through it.
//!
//! # Free when nobody looks
//!
//! The counters only move while an [`ObserveGuard`] is alive: a send costs
//! one relaxed load more than it did, and nothing runs, polls or allocates
//! on its own. Naming an inbox stores a weak reference once; a sample
//! is taken only when an observer asks for one, on the asker's thread.
//!
//! # Never the payload
//!
//! A sample names the kind of each queued message, through the function
//! the owner gave, and never holds or prints the message itself: a
//! mailbox may carry a password field's text or a file's bytes.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

static OBSERVERS: AtomicUsize = AtomicUsize::new(0);
static MAILBOXES: Mutex<Vec<Weak<dyn MailboxProbe>>> = Mutex::new(Vec::new());

/// Whether an observer is attached, so counters should move.
#[inline]
pub fn is_enabled() -> bool {
    OBSERVERS.load(Ordering::Relaxed) > 0
}

/// Turns the counters on until it is dropped. Guards nest: the counters
/// stop when the last one goes.
#[must_use = "the counters stop when the guard is dropped"]
#[derive(Debug)]
pub struct ObserveGuard(());

/// Starts observing; see [`ObserveGuard`].
pub fn enable() -> ObserveGuard {
    OBSERVERS.fetch_add(1, Ordering::Relaxed);
    ObserveGuard(())
}

impl Drop for ObserveGuard {
    fn drop(&mut self) {
        OBSERVERS.fetch_sub(1, Ordering::Relaxed);
    }
}

/// One mailbox as an observer sees it at one moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MailboxSample {
    /// The name its owner gave it.
    pub name: &'static str,
    pub capacity: usize,
    /// Messages waiting now.
    pub depth: usize,
    /// How long the front message has waited. `None` when the inbox is
    /// empty, or the front message was sent before anyone observed.
    pub oldest: Option<Duration>,
    /// Counted only while observed (see the module docs).
    pub sent: u64,
    /// Sends that found the inbox full and were refused as busy.
    pub refused: u64,
    /// Sends that replaced a queued message of the same key.
    pub replaced: u64,
    pub drained: u64,
    /// Sending handles alive.
    pub senders: usize,
    /// Senders waiting for room (`send_blocking`, `send_async`).
    pub waiting: usize,
    /// The queued messages by kind, most first.
    pub kinds: Vec<(&'static str, usize)>,
    /// The kinds of the first queued messages, front first.
    pub peek: Vec<&'static str>,
}

/// A mailbox that can be sampled from any thread.
pub trait MailboxProbe: Send + Sync {
    /// A sample, with the kinds of at most `peek` front messages.
    /// `None` once the receiving side is gone, even while senders live.
    fn sample(self: Arc<Self>, peek: usize) -> Option<MailboxSample>;
}

/// Adds a mailbox to the ones [`mailboxes`] samples. Kept weakly, so a
/// dropped inbox leaves on its own.
pub(crate) fn register(probe: Weak<dyn MailboxProbe>) {
    let mut all = MAILBOXES.lock().unwrap_or_else(|e| e.into_inner());
    all.retain(|w| w.strong_count() > 0);
    all.push(probe);
}

/// A sample of every named mailbox alive, in the order they were named.
pub fn mailboxes(peek: usize) -> Vec<MailboxSample> {
    // Upgraded under the lock and sampled outside it: a sample takes the
    // inbox's own lock, and an inbox being named meanwhile must not wait.
    let live: Vec<_> = {
        let mut all = MAILBOXES.lock().unwrap_or_else(|e| e.into_inner());
        all.retain(|w| w.strong_count() > 0);
        all.iter().filter_map(Weak::upgrade).collect()
    };
    live.into_iter().filter_map(|p| p.sample(peek)).collect()
}
