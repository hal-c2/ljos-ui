//! What clients are told about reloads.
//!
//! The transport has no channel the server may speak on by itself (HTTP
//! without a stream), so a reload is told three ways that all read this one
//! record: the `reload` block of `app_status`, the resource
//! `ljos://app/reload`, and `app_status` with `after_reload`, which the
//! server parks until a reload comes or the time given is up. The record is
//! a `watch` cell: a parked call sleeps on it, so waiting costs nothing.

use std::sync::Arc;

use ljos_compiler::CompileFault;
use serde_json::{Value, json};
use tokio::sync::watch;

use super::tools::clip;

/// How many warnings are listed, in how many bytes of JSON, and how long
/// each text may be in characters. An answer carries its value twice, so
/// the bytes are well under half of what a reply may have.
const MOST_WARNINGS: usize = 20;
const MOST_WARNING_BYTES: usize = 8 * 1024;
const MOST_MESSAGE_CHARS: usize = 1000;
const MOST_FILE_CHARS: usize = 256;

/// The compiler's warnings as clients are told them: bounded, and saying
/// when they were cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Warnings {
    listed: Vec<Value>,
    omitted: usize,
    truncated: bool,
}

impl Warnings {
    /// The first of `faults` by place, each text through `mask` and then
    /// cut, so that no cut leaves a part of a secret.
    pub(crate) fn bounded(faults: &[CompileFault], mask: impl Fn(&mut String)) -> Self {
        let mut sorted: Vec<&CompileFault> = faults.iter().collect();
        sorted.sort_by(|a, b| (&a.file, a.line, a.column).cmp(&(&b.file, b.line, b.column)));
        let mut listed = Vec::new();
        let mut truncated = false;
        // The array's brackets, then each entry with its comma.
        let mut bytes = 2;
        for fault in sorted.into_iter().take(MOST_WARNINGS) {
            let (mut file, mut message) = (fault.file.clone(), fault.message.clone());
            mask(&mut file);
            mask(&mut message);
            let entry = json!({
                "file": clip(&file, MOST_FILE_CHARS),
                "line": fault.line,
                "column": fault.column,
                "message": clip(&message, MOST_MESSAGE_CHARS),
            });
            bytes += entry.to_string().len() + 1;
            if bytes > MOST_WARNING_BYTES {
                break;
            }
            truncated |= file.chars().count() > MOST_FILE_CHARS
                || message.chars().count() > MOST_MESSAGE_CHARS;
            listed.push(entry);
        }
        let omitted = faults.len() - listed.len();
        Warnings {
            truncated: truncated || omitted > 0,
            omitted,
            listed,
        }
    }
}

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
    /// The compiler's warnings on what runs; none until a host that
    /// compiles says them, which is not the same as there being none.
    pub(crate) warnings: Option<Warnings>,
}

impl ReloadState {
    /// Puts the warnings' three members in the object `to`: the `reload`
    /// block, or the answer of a call that reloads.
    pub(crate) fn write_warnings(&self, to: &mut Value) {
        let known = self.warnings.as_ref();
        to["warnings"] = json!(known.map(|w| &w.listed));
        to["warnings_truncated"] = json!(known.is_some_and(|w| w.truncated));
        to["warnings_omitted"] = json!(known.map_or(0, |w| w.omitted));
    }

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
        let mut value = json!({
            "watched": self.watched,
            "generation": self.generation,
            "last": last,
            "aliases": self
                .aliases
                .iter()
                .map(|(module, dir)| json!({ "module": module, "dir": dir }))
                .collect::<Vec<_>>(),
            "waited": waited,
        });
        self.write_warnings(&mut value);
        value
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

    pub(crate) fn set_warnings(&self, warnings: Warnings) {
        self.tx.send_if_modified(|s| {
            let changed = s.warnings.as_ref() != Some(&warnings);
            s.warnings = Some(warnings);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fault(line: u32, file: &str, message: &str) -> CompileFault {
        CompileFault {
            file: file.to_owned(),
            line,
            column: 1,
            message: message.to_owned(),
        }
    }

    #[test]
    fn a_long_text_is_cut_and_marked() {
        let faults = [fault(1, &"f".repeat(300), &"m".repeat(1200))];
        let told = Warnings::bounded(&faults, |_| {});
        let one = &told.listed[0];
        assert_eq!(one["file"], format!("{}…", "f".repeat(MOST_FILE_CHARS)));
        assert_eq!(
            one["message"],
            format!("{}…", "m".repeat(MOST_MESSAGE_CHARS))
        );
        assert!(told.truncated);
        assert_eq!(told.omitted, 0);
    }

    #[test]
    fn the_list_stops_at_its_bytes() {
        // Three bytes a character, which a count of characters would miss.
        let text = "\u{20ac}".repeat(MOST_MESSAGE_CHARS);
        let faults: Vec<_> = (1..=10).map(|n| fault(n, "Main.qml", &text)).collect();
        let told = Warnings::bounded(&faults, |_| {});
        assert!(json!(told.listed).to_string().len() <= MOST_WARNING_BYTES);
        assert!(!told.listed.is_empty() && told.listed.len() < faults.len());
        assert_eq!(told.omitted, faults.len() - told.listed.len());
        assert!(told.truncated);
    }

    #[test]
    fn a_secret_at_the_cut_is_masked_before_it() {
        let text = format!("{}hunter2", "m".repeat(MOST_MESSAGE_CHARS - 3));
        let told = Warnings::bounded(&[fault(1, "Main.qml", &text)], |s| {
            *s = s.replace("hunter2", "*");
        });
        assert!(!told.listed[0].to_string().contains("hun"));
        assert!(!told.truncated);
    }
}
