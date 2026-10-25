use std::time::Duration;

/// Why a run of JS failed (ARCHITECTURE.md section 9.3).
///
/// `ljos_runtime` wraps or converts this into its own supervisor-facing
/// `JsFault`; the JS-specific variants are defined here because only this crate
/// can classify rquickjs errors.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum JsFault {
    #[error("JS exception: {message}{}", location(.file, .line))]
    JsException {
        message: String,
        /// The raw QuickJS stack text (JS positions, not mapped).
        stack: String,
        /// Module name, or the QML file when a [`SourceMapper`] knows it.
        file: Option<String>,
        line: Option<u32>,
        /// The value thrown: an `Error` or any object (by reference, as
        /// [`crate::JsValue::Handle`]) or a primitive. `None` where nothing
        /// was thrown by a script (a module that did not compile, a fault
        /// the embedder words as an exception).
        thrown: Option<crate::JsValue>,
    },
    /// `what` says where the dispatch was, the nested run the interrupt
    /// came in and the native work given back, as ` at Main.qml:7:5`,
    /// `, in ...`; it is empty where nothing is known.
    #[error("JS `{label}` interrupted after {after:?}{what}")]
    JsInterrupted {
        label: String,
        after: Duration,
        what: String,
    },
    #[error("JS heap limit exceeded in `{label}`")]
    JsOutOfMemory { label: String },
    #[error("panic in `{label}`: {message}")]
    Panic { label: String, message: String },
    /// A bug or misuse on the Rust side (stale handle, re-entrant `enter`,
    /// unsupported value shape), not a script failure.
    #[error("JS host: {0}")]
    Internal(String),
}

impl From<JsFault> for ljos_runtime::Fault {
    fn from(f: JsFault) -> Self {
        use ljos_runtime::{Fault, JsErrorInfo};
        match f {
            JsFault::JsException {
                message,
                stack,
                file,
                line,
                ..
            } => Fault::JsException(JsErrorInfo {
                message,
                file: file.map(Into::into),
                line: line.unwrap_or(0),
                column: 0,
                stack: (!stack.is_empty()).then_some(stack),
            }),
            JsFault::JsInterrupted { after, .. } => Fault::JsInterrupted { slice: after },
            JsFault::JsOutOfMemory { .. } => Fault::JsOutOfMemory,
            JsFault::Panic { label, message } => {
                Fault::Panic(format!("in `{label}`: {message}").into())
            }
            JsFault::Internal(m) => Fault::Internal(m),
        }
    }
}

fn location(file: &Option<String>, line: &Option<u32>) -> String {
    match (file, line) {
        (Some(f), Some(l)) => format!(" ({f}:{l})"),
        (Some(f), None) => format!(" ({f})"),
        _ => String::new(),
    }
}

impl From<rquickjs::Error> for JsFault {
    fn from(e: rquickjs::Error) -> Self {
        JsFault::Internal(e.to_string())
    }
}

/// Maps a JS position `(module name, line)` to the position in the source file
/// it was generated from. Source maps plug in here later; until then the
/// compiler's line table does not exist and the identity mapping applies.
pub type SourceMapper = Box<dyn Fn(&str, u32) -> Option<(String, u32)>>;

/// The text of a line `(module name, line)` of a module's source, read to
/// say what the script was doing where it threw (a call or a read).
pub type SourceText = Box<dyn Fn(&str, u32) -> Option<String>>;
