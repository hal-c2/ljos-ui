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
//! A visual type that paints itself and takes keys, the wheel and focus
//! is written against [`item`]: its paint, key and wheel hooks, the
//! display list it paints into and the text layout it may use.
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
/// value types, the derive and its `qml_impl` and `qml_singleton`
/// companions (with the [`Version`](prelude::Version) a singleton is
/// registered under), list and tree models for Rust-owned rows, the `Timer` a
/// host may run from Rust, the [`Item`](prelude::Item) a visual type extends
/// (`extends = Item` with a `#[qml(base)]` field), the platform
/// services (fetch, settings, dialogs, permissions, the paths, opening a
/// URL, the badge, the clipboard) a type may ask the host for, with what
/// a host's own service takes and gives and the drags from other
/// applications ([`ExternalDrag`](prelude::ExternalDrag)) a host hands
/// on, and what the Engine's own
/// methods hand out: the [`Time`](prelude::Time) of `Engine::now`, the
/// [`TimerId`](prelude::TimerId) of `Engine::call_at`, the
/// [`ConnectionId`](prelude::ConnectionId) of `Engine::connect`, the
/// [`MetaRef`](prelude::MetaRef) of `Engine::meta_of`, the
/// [`Prop`](prelude::Prop) handles a derive makes (`Counter::COUNT`), the
/// [`Application`](prelude::Application) singleton (`Qt.application`),
/// the [`StyleHints`](prelude::StyleHints) object of [`style_hints`]
/// and the [`ColorScheme`](prelude::ColorScheme) of [`color_scheme`];
/// and the standard locations ([`Location`](prelude::Location), Qt's
/// `QStandardPaths`) a [`PathEnv`](prelude::PathEnv) is read with.
///
/// A field's `#[qml(write = Self::name)]` is its setter: a
/// `fn(&self, T) -> Option<T>` every write goes through before it is
/// stored, from QML, a binding, an animation or Rust
/// (`Engine::write_by_name`, `Engine::set`). It returns the value to
/// store, which may be a clamped one, or `None` to refuse the write. It
/// runs for every write, one of the value already held included; nothing
/// hears of a write that leaves the value as it was.
///
/// A setter that has more to do, as a `WRITE` function in Qt may, takes
/// `fn(cx: &Cx, T) -> Option<T>` instead: `cx.this` is the object, and
/// what it writes to the object's other properties goes through their
/// own setters and is heard before the assignment returns. Writing its
/// own property from inside is refused with
/// [`EngineError::WriteInOwnSetter`](prelude::EngineError); what it
/// returns is what is stored.
///
/// ```
/// use ljos::prelude::*;
///
/// #[derive(Debug, Default, QmlObject)]
/// #[qml(module = "Example", name = "Dial")]
/// pub struct Dial {
///     #[qml(notify, write = Self::clamp_level)]
///     level: i32,
/// }
///
/// impl Dial {
///     fn clamp_level(&self, level: i32) -> Option<i32> {
///         Some(level.clamp(0, 10))
///     }
/// }
///
/// let engine = ljos::engine();
/// engine.register_module(ljos::module!("Example" 1.0 => [Dial]));
/// let dial = engine.create_by_name("Example", "Dial", None).unwrap();
/// engine.set(dial, Dial::LEVEL, 42).unwrap();
/// assert_eq!(engine.get(dial, Dial::LEVEL), Ok(10));
///
/// #[derive(Debug, Default, QmlObject)]
/// #[qml(module = "Example", name = "Browser")]
/// pub struct Browser {
///     #[qml(notify, write = Self::keep_root)]
///     root_path: Atom,
///     #[qml(notify)]
///     error: Atom,
/// }
///
/// impl Browser {
///     // Any write, the path already open included, clears the error.
///     fn keep_root(cx: &Cx, path: Atom) -> Option<Atom> {
///         cx.set(Self::ERROR, Atom::default()).ok()?;
///         Some(path)
///     }
/// }
///
/// engine.register_module(ljos::module!("Example" 1.0 => [Browser]));
/// let browser = engine.create_by_name("Example", "Browser", None).unwrap();
/// engine.set(browser, Browser::ERROR, Atom::from("denied")).unwrap();
/// engine.set(browser, Browser::ROOT_PATH, Atom::default()).unwrap();
/// assert_eq!(engine.get(browser, Browser::ERROR), Ok(Atom::default()));
/// ```
pub mod prelude {
    pub use crate::task::Task;
    pub use ljos_controls::{Application, ApplicationState};
    pub use ljos_engine::{
        Atom, Color, ConnectionId, ContextId, Cx, Engine, EngineError, Handler, ItemModel,
        ItemTree, JsError, LogEntry, LogLevel, MetaRef, ModelChange, ModelListener, ModelListeners,
        ModelSignals, ObjectId, Point, Prop, PropIndex, PropertyMap, QmlList, QmlListModel,
        QmlObject, QmlType, QmlValue, QtObject, Rect, RowKey, ScopedEvent, ScopedSingleton,
        SignalIndex, Size, Timer, TreeChange, TreeListener, TreeListeners, Url, Value, Version,
        WindowScope, list_model, qml_impl, qml_singleton, tree_model,
    };
    pub use ljos_platform::{
        BadgeService, ClipboardError, ClipboardImage, ClipboardMode, ClipboardService,
        DialogAnswer, DialogKind, DialogReply, DialogRequest, DialogService, ExternalDrag,
        FakeSensors, FetchError, Fetcher, Location, Measurement, Method, NameFilter, NoSensors,
        OpenUrlService, PathEnv, Permission, PermissionAnswer, PermissionService, PermissionStatus,
        Platform, RecordedBadge, RecordedUrls, Request, Response, SensorKind, SensorRequest,
        SensorService, SensorSink, StandardPaths, encoded_url, external_drag, file_url, is_remote,
        url_path,
    };
    pub use ljos_quick::{ColorScheme, Item, StyleHints};
    pub use ljos_runtime::{Time, TimerId};
}

mod app;
#[cfg(feature = "build")]
pub mod build;
#[cfg(any(feature = "dev-qml", feature = "build"))]
mod compile;
#[cfg(feature = "dev-qml")]
pub mod dev;
#[cfg(any(feature = "render", feature = "testing"))]
mod drive;
pub mod item;
pub mod locale;
#[cfg(feature = "mcp")]
pub mod mcp;
/// Hot reload in a development build (the `dev` feature): what
/// [`Setup::watch`](crate::Setup) and its kin tell
/// [`Setup::on_reload`](crate::Setup), and the type oracles
/// [`Setup::watch_with`](crate::Setup) compiles against.
#[cfg(feature = "dev")]
pub mod reload {
    pub use ljos_dev::{QtTypeOracle, TypeOracle};
    pub use ljos_shell::{Overlay, ReloadStats, Report};
}
/// Video a program makes or receives itself, shown by a `VideoOutput`,
/// a `Video` or a `VideoSink` (the `media` feature): a
/// [`media::VideoFrameInput`] takes frames from any thread and keeps the
/// newest. Sound plays through the system's output only with
/// `media-audio` (on by default); without it the sound library is not
/// linked and sound goes nowhere.
#[cfg(feature = "media")]
pub mod media {
    pub use ljos_media::{
        DecodeError, Decoder, FrameError, MediaStream, StreamError, StreamFormat, StreamOptions,
        VideoFrame, VideoFrameInput, VideoSink, YuvColour, YuvMatrix,
    };
}
#[cfg(feature = "render")]
pub mod render;
pub mod system;
pub mod task;
#[cfg(feature = "testing")]
pub mod testing;
pub mod types;

pub use app::{App, create_root};

/// The platform's own windows, for what only the platform's API does
/// (a window manager's hint, a native view added to the window):
/// [`App::on_window`] is handed each window the host opens, as a
/// [`NativeWindow`] whose handles are `raw-window-handle`'s, re-exported
/// here at the version the host uses.
///
/// [`replace_root`] puts another root in the window of the one shown, for
/// an application that makes its whole interface again.
pub mod window {
    pub use ljos_platform::{NativeWindow, raw_window_handle};
    pub use ljos_quick::roots::{RootError, replace_root};
}

/// Key events as a host delivers them, and the application-wide key
/// filters that see each before a window's shortcuts and focused item.
pub mod keys {
    pub use ljos_quick::keys::{KeyFilter, KeyFilterId, add_key_filter, key, remove_key_filter};
    pub use ljos_quick::{KeyEvent, KeyKind, Modifiers};
}

/// The components a build script packed with [`build::compile_dir`]
/// (the `build` feature, in `[build-dependencies]`), embedded in the
/// program: `include_qml!("qml.pack")` reads `$OUT_DIR/qml.pack` into a
/// [`MapSource`] for [`App::source`]. The program links no parser.
/// Panics only on a pack its own build did not write whole.
#[macro_export]
macro_rules! include_qml {
    ($name:literal) => {
        $crate::MapSource::from_pack(::core::include_bytes!(::core::concat!(
            ::core::env!("OUT_DIR"),
            "/",
            $name
        )))
        .expect("the QML pack this build wrote")
    };
}
/// The engine crate itself, for the paths the `QmlObject` derive,
/// `qml_impl` and `qml_singleton` generate in a crate that depends on
/// this facade alone (`::ljos::ljos_engine::…`); such a crate needs no
/// line of its own for them. Writing code against it directly bypasses
/// the curated surface.
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

/// The font families text is laid out with on this thread, sorted:
/// what `Qt.fontFamilies()` answers, for choosing a UI font from Rust
/// (a configured family, else the first of a list that is installed).
///
/// The answer depends on whether the text engine reads the system's
/// fonts, which [`run`] and [`App::run`] switch on
/// ([`ShellConfig::system_fonts`]) before anything of the application
/// runs. Ask from inside the application — a closure given to
/// [`App::register`], the setup closure ([`Setup::font_families`] is the
/// same) or later — not before `run`: there only the bundled font is
/// visible.
pub fn font_families() -> Vec<String> {
    ljos_quick::font_families()
}

/// The colour scheme the application follows: its own when it set one
/// (`Qt.styleHints.colorScheme`), else the system's. Tracked: a binding or
/// a native type's tracked read that asks follows a change of scheme.
pub fn color_scheme(engine: &Engine) -> prelude::ColorScheme {
    ljos_quick::color_scheme(engine)
}

/// `Qt.styleHints` (`Application.styleHints`): the object whose
/// [`StyleHints`](prelude::StyleHints) properties a native type reads,
/// and whose `colorScheme` it may write to choose the application's
/// scheme (`Qt.ColorScheme.Unknown` follows the system's again). `None`
/// only when the Engine cannot make it.
pub fn style_hints(engine: &Engine) -> Option<ObjectId> {
    ljos_quick::style_hints(engine)
}

/// Whether the user asked for less motion
/// (`Qt.styleHints.accessibility.motionPreference`). Tracked: a binding
/// or a native type's tracked read that asks follows the setting.
pub fn reduced_motion(engine: &Engine) -> bool {
    ljos_quick::reduced_motion(engine)
}

/// Makes `font` the application font while the application runs, or the
/// default again for `None`: every control, label and window that
/// inherits its font takes it, keeping what it or an object above it set,
/// and `Qt.application.font` reads it (QML cannot assign it, as in Qt). A
/// plain `Text` keeps the font it was made with.
///
/// ```
/// use ljos::item::Font;
///
/// let engine = ljos::engine();
/// let large = Font { pixel_size: 20, point_size: -1.0, ..Font::default() };
/// ljos::set_application_font(&engine, Some(&large));
/// assert_eq!(ljos::application_font(&engine).pixel_size, 20);
/// ljos::set_application_font(&engine, None);
/// assert_eq!(*ljos::application_font(&engine), Font::default());
/// ```
pub fn set_application_font(engine: &Engine, font: Option<&item::Font>) {
    ljos_controls::set_application_font(engine, font);
}

/// The application font. Tracked: a binding or a native type's tracked
/// read that asks follows [`set_application_font`].
pub fn application_font(engine: &Engine) -> std::sync::Arc<item::Font> {
    ljos_controls::application_font(engine)
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
/// fails fast on a full inbox with a [`SendError`]; `send_blocking`
/// and `send_async` ([`SendAsync`]) wait for room instead, for a
/// stream that must not lose messages. `send_coalesced` keeps only the
/// newest message under each [`CoalesceKey`].
#[cfg(feature = "shell")]
pub use ljos_runtime::{CoalesceKey, SendAsync, SendError, UiHandle};
#[cfg(feature = "shell")]
pub use ljos_shell::{Setup, SetupError, ShellConfig, ShellError};
