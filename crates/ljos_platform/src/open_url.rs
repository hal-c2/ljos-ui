//! Opening a URL in the application the user chose for it: a link in a
//! browser, a `mailto:` in the mail program, a file or folder in its
//! viewer (`Qt.openUrlExternally`).
//!
//! Without a host's service nothing is opened: [`RecordedUrls`] keeps what
//! was asked for and answers false, which is what tests and headless hosts
//! want. A desktop host turns on the `native` feature and installs
//! [`native::SystemOpener`](crate::native::SystemOpener).

use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{Cx, Engine, LogLevel, QmlObject, Value, qml_impl};

use crate::platform::{CATEGORY, Platform};

/// The system's handler of URLs, or a stand-in. `open` is called on the
/// Engine thread and must not wait for the application it starts: the
/// answer is whether the system took the URL, not whether it opened.
pub trait OpenUrlService {
    fn open(&self, url: &str) -> bool;
}

/// Opens nothing and keeps what it was asked to open, in order. Its
/// clones share one record, so a test keeps one and installs another.
#[derive(Clone, Debug, Default)]
pub struct RecordedUrls {
    urls: Rc<RefCell<Vec<String>>>,
}

impl RecordedUrls {
    /// Every URL asked for so far.
    pub fn urls(&self) -> Vec<String> {
        self.urls.borrow().clone()
    }
}

impl OpenUrlService for RecordedUrls {
    fn open(&self, url: &str) -> bool {
        self.urls.borrow_mut().push(url.to_owned());
        false
    }
}

/// What `Qt.openUrlExternally` calls; one per engine, made when scripts
/// start.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "OpenUrl", uncreatable)]
pub struct OpenUrl {}

#[qml_impl]
impl OpenUrl {
    #[qml(invokable)]
    fn open(cx: &Cx, url: Value) -> bool {
        let url = url.to_js_string();
        if url.is_empty() {
            return false;
        }
        Platform::of(cx.engine).open_url().open(&url)
    }
}

const INSTALL: &str = r#"(function (helper) {
  Object.defineProperty(Qt, "openUrlExternally", {
    value: function (url) { return helper.open(url); },
    enumerable: true,
  });
})"#;

pub(crate) fn register(engine: &Engine) {
    engine.on_js_started(|e| {
        let installed = e
            .create_native_incomplete(OpenUrl {}, None)
            .map_err(|err| err.to_string())
            .and_then(|helper| {
                let install = e.eval_js(INSTALL).map_err(|err| err.to_string())?;
                e.call_js(&install, &[Value::Object(helper)]);
                Ok(())
            });
        if let Err(err) = installed {
            e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("Qt.openUrlExternally is not available: {err}"),
            );
        }
    });
}
