//! The engine's log sink (contract item 6): warnings and console output
//! with a category, readable by tests and routed to a host handler.

use std::collections::VecDeque;
use std::fmt;

use crate::engine::Engine;

/// Entries kept when no handler is set.
pub(crate) const MAX_LOG: usize = 1024;

/// Severity, as Qt's message types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Critical,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warning => "warning",
            LogLevel::Critical => "critical",
        })
    }
}

/// One message. `category` is Qt's logging category: `qml` for
/// `console.log` and engine warnings, as `qml6` prints them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub category: String,
    pub message: String,
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.category, self.message)
    }
}

pub(crate) type LogHandler = Box<dyn FnMut(&LogEntry)>;
pub(crate) type WarningHandler = Box<dyn FnMut(&str)>;

/// Says which listener to remove ([`Engine::remove_log_listener`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogListenerId(u64);

#[derive(Default)]
pub(crate) struct LogState {
    pub(crate) entries: VecDeque<LogEntry>,
    pub(crate) handler: Option<LogHandler>,
    pub(crate) warning_handler: Option<WarningHandler>,
    listeners: Vec<(LogListenerId, LogHandler)>,
    next_listener: u64,
}

impl Engine {
    /// Logs a message. With a handler set ([`Engine::set_log_handler`]) it
    /// goes there; otherwise the last 1024 are kept for
    /// [`Engine::take_log`].
    pub fn log(&self, level: LogLevel, category: &str, message: impl Into<String>) {
        let entry = LogEntry {
            level,
            category: category.to_owned(),
            message: message.into(),
        };
        self.tell_listeners(&entry);
        if entry.level >= LogLevel::Warning {
            // Taken out while it runs, so a handler that logs does not
            // re-borrow.
            let wh = self.inner.log.borrow_mut().warning_handler.take();
            if let Some(mut h) = wh {
                h(&entry.message);
                let mut log = self.inner.log.borrow_mut();
                if log.warning_handler.is_none() {
                    log.warning_handler = Some(h);
                }
                return;
            }
        }
        let handler = self.inner.log.borrow_mut().handler.take();
        match handler {
            Some(mut h) => {
                h(&entry);
                let mut log = self.inner.log.borrow_mut();
                if log.handler.is_none() {
                    log.handler = Some(h);
                }
            }
            None => {
                let mut log = self.inner.log.borrow_mut();
                if log.entries.len() == MAX_LOG {
                    log.entries.pop_front();
                }
                log.entries.push_back(entry);
            }
        }
    }

    /// Taken out while they run, like the handlers: what a listener logs
    /// goes on to the handler and not back to the listeners.
    fn tell_listeners(&self, entry: &LogEntry) {
        let mut listeners = std::mem::take(&mut self.inner.log.borrow_mut().listeners);
        if listeners.is_empty() {
            return;
        }
        for (_, listener) in &mut listeners {
            listener(entry);
        }
        let mut log = self.inner.log.borrow_mut();
        listeners.append(&mut log.listeners);
        log.listeners = listeners;
    }

    /// Lets `f` hear every log entry, as well as whatever else hears it:
    /// the handler ([`Engine::set_log_handler`]) or the buffer, and any
    /// other listener. For a tool that watches an application, whose view
    /// must not depend on what the application does with its log.
    pub fn add_log_listener(&self, f: impl FnMut(&LogEntry) + 'static) -> LogListenerId {
        let mut log = self.inner.log.borrow_mut();
        log.next_listener += 1;
        let id = LogListenerId(log.next_listener);
        log.listeners.push((id, Box::new(f)));
        id
    }

    /// Stops a listener hearing the log. Not from inside a listener, which
    /// is not among them while it runs.
    pub fn remove_log_listener(&self, id: LogListenerId) {
        self.inner
            .log
            .borrow_mut()
            .listeners
            .retain(|(i, _)| *i != id);
    }

    /// Routes every log entry to `f` instead of the buffer. There is one
    /// handler; listeners ([`Engine::add_log_listener`]) hear the entries
    /// whatever it is.
    pub fn set_log_handler(&self, f: impl FnMut(&LogEntry) + 'static) {
        self.inner.log.borrow_mut().handler = Some(Box::new(f));
    }

    /// Removes the handler; entries are buffered again.
    pub fn clear_log_handler(&self) {
        self.inner.log.borrow_mut().handler = None;
    }

    /// Drains the buffered entries.
    pub fn take_log(&self) -> Vec<LogEntry> {
        self.inner.log.borrow_mut().entries.drain(..).collect()
    }

    /// The buffered entries, without draining them.
    pub fn log_snapshot(&self) -> Vec<LogEntry> {
        self.inner.log.borrow().entries.iter().cloned().collect()
    }

    /// Routes warnings (binding loops, failed assignments) to `f` as text.
    /// Other levels still go to the log handler or the buffer.
    pub fn set_warning_handler(&self, f: impl FnMut(&str) + 'static) {
        self.inner.log.borrow_mut().warning_handler = Some(Box::new(f));
    }

    /// Drains buffered warnings (and criticals) as text; other entries stay.
    pub fn take_warnings(&self) -> Vec<String> {
        let mut log = self.inner.log.borrow_mut();
        let (warn, rest): (Vec<_>, Vec<_>) = log
            .entries
            .drain(..)
            .partition(|e| e.level >= LogLevel::Warning);
        log.entries = rest.into();
        warn.into_iter().map(|e| e.message).collect()
    }

    /// An engine warning in category `qml`, held instead while a binding
    /// whose warnings are held runs ([`Engine::run_binding`]).
    pub fn warn(&self, msg: String) {
        if self.hold_warning(&msg) {
            return;
        }
        self.log(LogLevel::Warning, "qml", msg);
    }
}
