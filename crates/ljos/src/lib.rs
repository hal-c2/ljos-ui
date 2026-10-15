//! Ljós UI: a QML runtime in Rust, and the facade an application is
//! written against.
//!
//! Everything an application needs is here behind a small interface:
//! [`App`] is the start of an application (its modules, its compiled
//! components, its root, its windows), the [`prelude`] has the types a
//! native QML type is written with, and [`engine`] makes an [`Engine`]
//! with the standard modules registered for a host that assembles its
//! own start. With the `shell` feature, on by default, [`run`] is the
//! shell's loop under the builder. The workspace crates behind this
//! facade (`ljos_engine`, `ljos_quick`, `ljos_controls`,
//! `ljos_platform`, `ljos_shell`) stay usable directly, but their full
//! surfaces are internal in spirit: what is not re-exported here may
//! change without notice.
//!
//! A native type an application registers looks like this (see
//! `docs/MIGRATION.md` section 6 for the longer walk-through):
//!
//! ```
//! use ljos::prelude::*;
//!
//! #[derive(Default, QmlObject)]
//! #[qml(module = "Example", name = "Counter")]
//! pub struct Counter {
//!     #[qml(notify)]
//!     pub count: i32,
//! }
//!
//! #[qml_impl]
//! impl Counter {
//!     #[qml(signal)]
//!     fn bumped();
//!
//!     #[qml(invokable)]
//!     fn increment(cx: &Cx) -> Result<(), EngineError> {
//!         let n = cx.get(Counter::COUNT)?;
//!         cx.set(Counter::COUNT, n + 1)
//!     }
//! }
//!
//! pub fn register(engine: &Engine) {
//!     engine.register_module(ljos::module!("Example" 1.0 => [Counter]));
//! }
//!
//! let engine = ljos::engine();
//! register(&engine);
//! let counter = engine.create_by_name("Example", "Counter", None).unwrap();
//! engine.invoke_by_name(counter, "increment", &[]).unwrap();
//! assert_eq!(engine.get(counter, Counter::COUNT), Ok(1));
//! ```
//!
//! The `register` function is the seam every native module sits at: the
//! toolkit's own modules register the same way, and a host (the shell, a
//! test, the gallery renderer) calls each module's `register` on the
//! Engine it made. Nothing else about a module needs to be public.
//!
//! Network access is a host service, reached as
//! `Platform::fetch(url, done)` ([`prelude::Platform`]): the fetch
//! runs on a thread of its own and `done` runs on the Engine thread.
//! The desktop shell installs an HTTPS-capable fetcher, so an
//! application run through [`App`] or [`run`] can fetch. A headless
//! host — a test, a renderer, another shell — installs its own with
//! `Platform::of(engine).set_fetcher(..)`, or keeps the default,
//! under which every fetch fails with
//! [`FetchError::NoNetwork`](prelude::FetchError).

/// What a native type is written with: the Engine and object model, the
/// value types, the derive and its `qml_impl` companion, list models for
/// Rust-owned rows, and the platform services (fetch, settings, dialogs)
/// a type may ask the host for.
pub mod prelude {
    pub use crate::task::Task;
    pub use ljos_engine::{
        Atom, Color, ContextId, Cx, Engine, EngineError, Handler, ItemModel, LogEntry, LogLevel,
        ModelChange, ModelSignals, ObjectId, Point, PropertyMap, QmlList, QmlListModel, QmlObject,
        QmlType, QmlValue, QtObject, Rect, RowKey, SignalIndex, Size, Url, Value, list_model,
        qml_impl,
    };
    pub use ljos_platform::{
        FetchError, Fetcher, Method, Platform, Request, Response, is_remote, url_path,
    };
}

mod app;
#[cfg(feature = "dev-qml")]
pub mod dev;
pub mod task;

pub use app::{App, create_root};
/// The engine crate itself, for the paths the `QmlObject` derive and
/// `qml_impl` generate (`::ljos_engine::…`): an application crate that
/// depends on this facade alone re-exports it at its own root
/// (`pub use ljos::ljos_engine;`) so the generated paths resolve.
/// Writing code against it directly bypasses the curated surface.
#[doc(hidden)]
pub use ljos_engine;
#[doc(hidden)]
pub use ljos_engine::module;
pub use ljos_engine::{ComponentSource, Engine, MapSource, ObjectId};

/// An Engine with the standard modules registered (QtQml, QtQuick,
/// QtQuick.Controls, with `media` QtMultimedia, and what they pull in), ready for an application's
/// own `register` functions and its root component.
pub fn engine() -> Engine {
    let engine = Engine::new();
    ljos_quick::register(&engine);
    ljos_controls::register(&engine);
    #[cfg(feature = "media")]
    ljos_media::register(&engine);
    engine
}

/// Opens the application's windows and runs until the last of them is
/// closed, on the desktop shell. `setup` receives the shell's setup
/// (the Engine — already holding the standard modules — and the
/// thread-safe handle worker threads post messages through) and builds
/// the root item.
///
/// Call it on the main thread: some platforms allow a loop nowhere else.
#[cfg(feature = "shell")]
pub fn run<M: Send + 'static>(
    config: ljos_shell::ShellConfig,
    setup: impl FnOnce(&mut ljos_shell::Setup<'_, M>) -> Result<ObjectId, ljos_shell::SetupError>
    + 'static,
) -> Result<(), ljos_shell::ShellError> {
    ljos_shell::run(config, setup)
}

/// The thread-safe handle a worker thread posts messages to the
/// Engine through: what [`Setup::handle`] hands out. Its `send`
/// fails fast on a full inbox with a [`SendError`].
#[cfg(feature = "shell")]
pub use ljos_runtime::{SendError, UiHandle};
#[cfg(feature = "shell")]
pub use ljos_shell::{Setup, SetupError, ShellConfig, ShellError};
