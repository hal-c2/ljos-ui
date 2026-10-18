//! Which client holds which traces, on the server's thread.
//!
//! Each call is answered on its own, with no session to end, so a client
//! is known to be there only while it waits for the application with
//! `app_watch`. A client that follows the application asks again as soon
//! as it is answered. One that has not asked again within the time its
//! last watch said it would wait (at least [`LEAST`], at most [`MOST`])
//! is gone, as is one whose watch was dropped before it was answered: its
//! connection closed. The traces it holds are then stopped.
//!
//! Nothing here runs while nobody holds anything: the one timer is armed
//! when a holder's last watch ends, and the UI thread is woken only to
//! stop traces that a gone holder held.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The least time a holder has to ask again.
pub(crate) const LEAST: Duration = Duration::from_secs(1);
/// The most: a watch may wait an hour, and its holder, gone right after
/// it was answered, would hold its traces for as long again.
pub(crate) const MOST: Duration = Duration::from_secs(30);
/// The time a holder that has never watched has to start.
pub(crate) const FIRST: Duration = Duration::from_millis(super::watch::DEFAULT_WAIT_MS);

type Release = Box<dyn Fn(String) + Send + Sync>;

pub(crate) struct Holders {
    by_name: Mutex<HashMap<String, Holder>>,
    /// Stops the traces a gone holder held, on the UI thread.
    release: Release,
}

#[derive(Default)]
struct Holder {
    /// Watches of this holder waiting now.
    waiting: usize,
    /// Moved by each watch that starts or ends, so that a timer armed
    /// before knows it is out of date.
    epoch: u64,
    /// Whether it was ever given a trace; one that was not is forgotten
    /// without waking the UI thread.
    holds: bool,
}

impl Holders {
    pub(crate) fn new(release: impl Fn(String) + Send + Sync + 'static) -> Arc<Holders> {
        Arc::new(Holders {
            by_name: Mutex::new(HashMap::new()),
            release: Box::new(release),
        })
    }

    /// `name` was given a trace.
    pub(crate) fn holds(self: &Arc<Self>, name: &str) {
        let mut by_name = self.lock();
        let h = by_name.entry(name.to_owned()).or_default();
        h.holds = true;
        if h.waiting == 0 {
            h.epoch += 1;
            self.arm(name.to_owned(), h.epoch, FIRST);
        }
    }

    /// `name` waits for the application for at most `wait`. The watch
    /// says it was answered with [`Watching::answered`]; dropped without,
    /// its holder is gone.
    pub(crate) fn watching(self: &Arc<Self>, name: &str, wait: Duration) -> Watching {
        let mut by_name = self.lock();
        let h = by_name.entry(name.to_owned()).or_default();
        h.waiting += 1;
        h.epoch += 1;
        Watching {
            holders: self.clone(),
            name: name.to_owned(),
            grace: wait.clamp(LEAST, MOST),
            answered: false,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Holder>> {
        self.by_name.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn ended(self: &Arc<Self>, name: &str, grace: Duration, answered: bool) {
        let mut by_name = self.lock();
        let Some(h) = by_name.get_mut(name) else {
            return;
        };
        h.waiting = h.waiting.saturating_sub(1);
        h.epoch += 1;
        if h.waiting > 0 {
            return;
        }
        if answered {
            let epoch = h.epoch;
            self.arm(name.to_owned(), epoch, grace);
        } else if let Some(h) = by_name.remove(name) {
            drop(by_name);
            self.gone(name.to_owned(), h);
        }
    }

    /// Forgets `name` after `after` unless it watched meanwhile.
    fn arm(self: &Arc<Self>, name: String, epoch: u64, after: Duration) {
        // Without a runtime (the server is ending) there is nobody left
        // to hold anything.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let holders = Arc::downgrade(self);
        runtime.spawn(async move {
            tokio::time::sleep(after).await;
            let Some(holders) = holders.upgrade() else {
                return;
            };
            let mut by_name = holders.lock();
            let still = by_name
                .get(&name)
                .is_some_and(|h| h.waiting == 0 && h.epoch == epoch);
            if still && let Some(h) = by_name.remove(&name) {
                drop(by_name);
                holders.gone(name, h);
            }
        });
    }

    fn gone(&self, name: String, h: Holder) {
        if h.holds {
            (self.release)(name);
        }
    }
}

/// One watch of a holder, from when it starts until it is answered or
/// dropped.
pub(crate) struct Watching {
    holders: Arc<Holders>,
    name: String,
    grace: Duration,
    answered: bool,
}

impl Watching {
    pub(crate) fn answered(mut self) {
        self.answered = true;
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.holders.ended(&self.name, self.grace, self.answered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holders() -> (Arc<Holders>, Arc<Mutex<Vec<String>>>) {
        let released = Arc::new(Mutex::new(Vec::new()));
        let r = released.clone();
        let holders = Holders::new(move |name| r.lock().unwrap().push(name));
        (holders, released)
    }

    #[tokio::test(start_paused = true)]
    async fn a_holder_that_asks_again_in_time_keeps_its_traces() {
        let (holders, released) = holders();
        holders.holds("a");
        for _ in 0..3 {
            holders.watching("a", Duration::from_secs(2)).answered();
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        assert!(released.lock().unwrap().is_empty());
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(*released.lock().unwrap(), ["a"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_holder_whose_watch_is_dropped_is_gone_at_once() {
        let (holders, released) = holders();
        holders.holds("a");
        drop(holders.watching("a", Duration::from_secs(30)));
        assert_eq!(*released.lock().unwrap(), ["a"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_holder_that_never_watches_is_gone_after_the_first_wait() {
        let (holders, released) = holders();
        holders.holds("a");
        tokio::time::sleep(FIRST - Duration::from_millis(1)).await;
        assert!(released.lock().unwrap().is_empty());
        tokio::time::sleep(Duration::from_millis(2)).await;
        assert_eq!(*released.lock().unwrap(), ["a"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_watcher_that_holds_nothing_wakes_nobody() {
        let (holders, released) = holders();
        drop(holders.watching("a", Duration::from_secs(1)));
        holders.watching("b", Duration::from_secs(1)).answered();
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(released.lock().unwrap().is_empty());
        assert!(holders.lock().is_empty());
    }
}
