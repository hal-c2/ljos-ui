//! Warnings from this crate.
//!
//! SHIM for contract item 6: the engine's log sink with categories does
//! not exist yet, and `Engine::warn` is crate-private. Entries collect here
//! and in an optional handler; when the sink lands, [`QuickLog::warn`]
//! forwards to it and this type goes away.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// One warning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// `qt.quick.anchors`, `qt.quick.polish` …
    pub category: &'static str,
    pub message: String,
}

const MAX_ENTRIES: usize = 1024;

type Handler = Box<dyn FnMut(&LogEntry)>;

/// A shared, bounded warning log.
#[derive(Clone, Default)]
pub struct QuickLog {
    inner: Rc<RefCell<LogInner>>,
}

#[derive(Default)]
struct LogInner {
    entries: VecDeque<LogEntry>,
    handler: Option<Handler>,
}

impl std::fmt::Debug for QuickLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickLog")
            .field("entries", &self.inner.borrow().entries.len())
            .finish()
    }
}

impl QuickLog {
    pub fn warn(&self, category: &'static str, message: String) {
        let entry = LogEntry { category, message };
        let handler = self.inner.borrow_mut().handler.take();
        match handler {
            Some(mut h) => {
                h(&entry);
                let mut i = self.inner.borrow_mut();
                if i.handler.is_none() {
                    i.handler = Some(h);
                }
            }
            None => {
                let mut i = self.inner.borrow_mut();
                if i.entries.len() == MAX_ENTRIES {
                    i.entries.pop_front();
                }
                i.entries.push_back(entry);
            }
        }
    }

    /// Sends future warnings to `f` instead of keeping them.
    pub fn set_handler(&self, f: impl FnMut(&LogEntry) + 'static) {
        self.inner.borrow_mut().handler = Some(Box::new(f));
    }

    pub fn take(&self) -> Vec<LogEntry> {
        self.inner.borrow_mut().entries.drain(..).collect()
    }
}
