//! The side streams a scenario made (features/runtime/batch_streams.feature),
//! fed from the step thread and handled as the UI thread would: they hang
//! off an inbox of their own whose wake counts how often it fired.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ljos_runtime::{
    BatchPolicy, BatchSender, CoalesceKey, DrainBudget, SystemClock, UiBatch, UiHandle, UiInbox,
};

/// One stream of text items and what its handler was handed.
struct Stream {
    batch: UiBatch<String>,
    sender: BatchSender<String>,
    /// Runs the handler was called with.
    turns: usize,
    /// Every item handed over, in order.
    handed: Vec<String>,
}

/// The inbox the scenario's streams wake, made with its first stream.
struct Ui {
    // Kept so the handle's streams have an inbox to wake.
    _inbox: UiInbox<()>,
    handle: UiHandle<()>,
    woken: Arc<AtomicUsize>,
}

#[derive(Default)]
pub struct SideStreams {
    ui: Option<Ui>,
    streams: BTreeMap<String, Stream>,
}

impl fmt::Debug for SideStreams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SideStreams")
            .field("streams", &self.streams.keys().collect::<Vec<_>>())
            .field("woken", &self.woken())
            .finish()
    }
}

/// An item `key=value` is coalesced by its key; any other by all of it.
#[allow(
    clippy::ptr_arg,
    reason = "a coalescing key is a `fn(&T)` of the stream's item type, here `String`"
)]
fn key_of(item: &String) -> CoalesceKey {
    let key = item.split_once('=').map_or(item.as_str(), |(k, _)| k);
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    CoalesceKey(h.finish())
}

impl SideStreams {
    /// A stream named `name` of `capacity` items with the policy the
    /// step's words name.
    pub fn make(&mut self, name: &str, capacity: usize, policy: &str) -> Result<(), String> {
        let policy = match policy {
            "keeps the newest per key" => BatchPolicy::Coalesce(key_of),
            "refuses the newest when full" => BatchPolicy::DropNewest,
            "evicts the oldest when full" => BatchPolicy::DropOldest,
            other => return Err(format!("{other:?} is not a policy this step knows")),
        };
        let ui = self.ui.get_or_insert_with(|| {
            let woken = Arc::new(AtomicUsize::new(0));
            let count = Arc::clone(&woken);
            let (inbox, handle) = UiInbox::channel_with_wake(
                1,
                Arc::new(move || {
                    count.fetch_add(1, Ordering::SeqCst);
                }),
            );
            Ui {
                _inbox: inbox,
                handle,
                woken,
            }
        });
        // A stream's name is `'static` as an application's are; a
        // scenario makes a handful.
        let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
        let (batch, sender) = ui.handle.batch(leaked, capacity, policy);
        let stream = Stream {
            batch,
            sender,
            turns: 0,
            handed: Vec::new(),
        };
        self.streams.insert(name.to_owned(), stream);
        Ok(())
    }

    fn stream(&mut self, name: &str) -> Result<&mut Stream, String> {
        self.streams
            .get_mut(name)
            .ok_or_else(|| format!("no stream {name:?} was made"))
    }

    /// Sends each item; one the stream refuses is counted by it, not an error.
    pub fn send<I: IntoIterator<Item = String>>(
        &mut self,
        name: &str,
        items: I,
    ) -> Result<(), String> {
        let stream = self.stream(name)?;
        for item in items {
            match stream.sender.send(item) {
                Ok(()) | Err(ljos_runtime::SendError::Full(_)) => {}
                Err(e) => return Err(format!("the stream {name:?} refused: {e}")),
            }
        }
        Ok(())
    }

    /// One turn of the UI thread for the stream, as the shell runs it.
    pub fn handle(&mut self, name: &str, budget: DrainBudget) -> Result<(), String> {
        let stream = self.stream(name)?;
        let Stream {
            batch,
            turns,
            handed,
            ..
        } = stream;
        batch.drain(budget, &SystemClock::new(), |run| {
            *turns += 1;
            handed.extend(run);
        });
        Ok(())
    }

    pub fn woken(&self) -> usize {
        self.ui
            .as_ref()
            .map_or(0, |ui| ui.woken.load(Ordering::SeqCst))
    }

    /// The turns the stream was handled in and the items they handed over.
    pub fn handled(&mut self, name: &str) -> Result<(usize, &[String]), String> {
        let stream = self.stream(name)?;
        Ok((stream.turns, &stream.handed))
    }

    pub fn holds(&mut self, name: &str) -> Result<usize, String> {
        Ok(self.stream(name)?.batch.len())
    }
}
