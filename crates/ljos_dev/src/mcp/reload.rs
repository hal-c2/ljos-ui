//! What clients are told about reloads.
//!
//! The transport has no channel the server may speak on by itself (HTTP
//! without a stream), so a reload is told three ways that all read this one
//! record: the `reload` block of `app_status`, the resource
//! `ljos://app/reload`, and `app_status` with `after_reload`, which the
//! server parks until a reload comes or the time given is up. The record is
//! a `watch` cell: a parked call sleeps on it, so waiting costs nothing.

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::watch;

/// How the last reload went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub(crate) applied: bool,
    /// Why it failed; empty for an applied one.
    pub(crate) message: String,
    /// The generation after it.
    pub(crate) generation: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReloadState {
    /// Whether the host watches the files at all.
    pub(crate) watched: bool,
    /// The engine's reload generation.
    pub(crate) generation: u64,
    /// Counts reloads, applied or failed, so that a failure is an event even
    /// though the generation does not move.
    pub(crate) seq: u64,
    pub(crate) last: Option<Outcome>,
    /// The modules imported from a directory of their own, by module, as
    /// the host compiles against them: each module and its directory.
    pub(crate) aliases: Vec<(String, String)>,
}

impl ReloadState {
    /// The `reload` block of `app_status`. `waited` is whether the call
    /// had to wait for it.
    pub(crate) fn json(&self, waited: bool) -> Value {
        let last = self.last.as_ref().map(|o| {
            let mut v = json!({
                "outcome": if o.applied { "applied" } else { "failed" },
                "generation": o.generation,
            });
            if !o.applied {
                v["message"] = json!(o.message);
            }
            v
        });
        json!({
            "watched": self.watched,
            "generation": self.generation,
            "last": last,
            "aliases": self
                .aliases
                .iter()
                .map(|(module, dir)| json!({ "module": module, "dir": dir }))
                .collect::<Vec<_>>(),
            "waited": waited,
        })
    }
}

/// Written by the inspector on the UI thread, read by the server's thread.
#[derive(Debug)]
pub(crate) struct ReloadFeed {
    tx: watch::Sender<ReloadState>,
}

impl ReloadFeed {
    pub(crate) fn new() -> Arc<ReloadFeed> {
        let (tx, _) = watch::channel(ReloadState::default());
        Arc::new(ReloadFeed { tx })
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<ReloadState> {
        self.tx.subscribe()
    }

    pub(crate) fn state(&self) -> ReloadState {
        self.tx.borrow().clone()
    }

    pub(crate) fn set_watched(&self, watched: bool) {
        self.tx.send_if_modified(|s| {
            let changed = s.watched != watched;
            s.watched = watched;
            changed
        });
    }

    pub(crate) fn set_aliases(&self, aliases: Vec<(String, String)>) {
        self.tx.send_if_modified(|s| {
            let changed = s.aliases != aliases;
            s.aliases = aliases;
            changed
        });
    }

    /// The generation the engine had when the inspector attached.
    pub(crate) fn baseline(&self, generation: u64) {
        self.tx.send_modify(|s| s.generation = generation);
    }

    /// The engine's generation moved (a reload was applied).
    pub(crate) fn applied(&self, generation: u64) {
        self.tx.send_modify(|s| {
            s.generation = generation;
            s.seq += 1;
            s.last = Some(Outcome {
                applied: true,
                message: String::new(),
                generation,
            });
        });
    }

    /// A reload was tried and did not take; the application goes on as it was.
    pub(crate) fn failed(&self, message: String) {
        self.tx.send_modify(|s| {
            s.seq += 1;
            s.last = Some(Outcome {
                applied: false,
                message,
                generation: s.generation,
            });
        });
    }
}
