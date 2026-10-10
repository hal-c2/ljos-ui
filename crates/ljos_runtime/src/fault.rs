//! Failures that reach the runtime (ARCHITECTURE.md 9.3).
//!
//! These are data, not control flow: a job or handler that fails reports a
//! [`Fault`], and the supervisor decides what to restart.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

/// A JS exception with the QML source location it came from, so the log points
/// at the `.qml` line rather than at the engine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JsErrorInfo {
    pub message: String,
    pub file: Option<Arc<str>>,
    pub line: u32,
    pub column: u32,
    pub stack: Option<String>,
}

impl std::fmt::Display for JsErrorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{file}:{}:{}: {}", self.line, self.column, self.message),
            None => f.write_str(&self.message),
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Fault {
    /// A JS exception in a handler. Other handlers keep running.
    #[error("JS exception: {0}")]
    JsException(JsErrorInfo),
    /// The interrupt handler cut a JS entry that exceeded its slice.
    #[error("JS interrupted after exceeding its {slice:?} slice")]
    JsInterrupted { slice: Duration },
    #[error("JS out of memory")]
    JsOutOfMemory,
    /// A Rust panic caught at a turn boundary (needs `panic = "unwind"`).
    #[error("panic: {0}")]
    Panic(Cow<'static, str>),
    /// A child failed to start.
    #[error("startup failed: {0}")]
    Startup(String),
    /// A bug or misuse on the Rust side of a host (a stale handle, a
    /// re-entrant entry), not a failure of the code it ran.
    #[error("internal: {0}")]
    Internal(String),
}

impl Fault {
    /// Converts a `catch_unwind` payload into a [`Fault::Panic`].
    pub fn from_panic(payload: Box<dyn std::any::Any + Send>) -> Fault {
        let msg = match payload.downcast::<&'static str>() {
            Ok(s) => Cow::Borrowed(*s),
            Err(payload) => match payload.downcast::<String>() {
                Ok(s) => Cow::Owned(*s),
                Err(_) => Cow::Borrowed("panic with a non-string payload"),
            },
        };
        Fault::Panic(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_payloads_become_messages() {
        let p = std::panic::catch_unwind(|| panic!("boom")).unwrap_err();
        assert_eq!(Fault::from_panic(p), Fault::Panic("boom".into()));
        let p = std::panic::catch_unwind(|| panic!("n={}", 3)).unwrap_err();
        assert_eq!(Fault::from_panic(p), Fault::Panic("n=3".into()));
        let p = std::panic::catch_unwind(|| std::panic::panic_any(5u8)).unwrap_err();
        assert!(matches!(Fault::from_panic(p), Fault::Panic(_)));
    }

    #[test]
    fn js_exception_display_has_location() {
        let f = Fault::JsException(JsErrorInfo {
            message: "x is not defined".into(),
            file: Some("Main.qml".into()),
            line: 12,
            column: 4,
            stack: None,
        });
        assert_eq!(
            f.to_string(),
            "JS exception: Main.qml:12:4: x is not defined"
        );
    }
}
