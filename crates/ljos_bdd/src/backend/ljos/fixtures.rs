//! `import Fixtures`: native types an application would register, for
//! scenarios about what the engine does with such types. No scenario that
//! does not import the module sees them.

use ljos::prelude::*;
use ljos_engine::{
    Engine, EngineError, LogLevel, ObjectId, QmlObject, ScopedEvent, Version, qml_singleton,
};

/// One for each window (`features/language/singletons-per-window.feature`).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Fixtures", name = "Document")]
#[qml_singleton(scope = "window")]
pub(super) struct Document {
    #[qml(notify)]
    pub title: String,
}

/// One for the engine.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Fixtures", name = "Session")]
#[qml_singleton]
pub(super) struct Session {
    #[qml(notify)]
    pub title: String,
}

/// Setters that do more than store their value, as a WRITE function in Qt
/// may (`features/language/property-setter-writes-others.feature`).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Fixtures", name = "Browser", extends = QtObject)]
pub(super) struct Browser {
    #[qml(base)]
    object: QtObject,
    /// Clears `error` on every write and counts it in `visits`.
    #[qml(notify, write = Self::keep_root)]
    root_path: String,
    #[qml(notify)]
    error: String,
    /// Kept at 3 or below by a setter of its own.
    #[qml(notify, write = Self::keep_visits)]
    visits: i32,
    /// Clears `error` on every write.
    #[qml(notify, write = Self::keep_zoom)]
    zoom: f64,
    /// Tries to write itself, and logs what it is told.
    #[qml(notify, write = Self::keep_depth)]
    depth: i32,
}

impl Browser {
    fn keep_root(cx: &Cx, path: String) -> Option<String> {
        cx.set(Self::ERROR, String::new()).ok()?;
        let visits = cx.with(|b: &Self| b.visits).ok()?;
        cx.set(Self::VISITS, visits + 1).ok()?;
        Some(path)
    }

    fn keep_visits(&self, v: i32) -> Option<i32> {
        Some(v.min(3))
    }

    fn keep_zoom(cx: &Cx, zoom: f64) -> Option<f64> {
        cx.set(Self::ERROR, String::new()).ok()?;
        Some(zoom)
    }

    fn keep_depth(cx: &Cx, depth: i32) -> Option<i32> {
        if let Err(e) = cx.set(Self::DEPTH, depth + 1) {
            cx.engine.log(LogLevel::Warning, "qml", e.to_string());
        }
        Some(depth)
    }
}

/// A `Document` the application made itself, titled `title`.
pub(super) fn document(engine: &Engine, title: &str) -> Result<ObjectId, EngineError> {
    engine.create_native(
        Document {
            title: title.to_owned(),
        },
        None,
    )
}

/// The title of the `Document` `obj`.
pub(super) fn title(engine: &Engine, obj: ObjectId) -> Result<String, EngineError> {
    engine.with::<Document, _>(obj, |d| d.title.clone())
}

pub(super) fn register(engine: &Engine) {
    engine.register_singleton::<Document>("Fixtures", Version::new(1, 0));
    engine.register_singleton::<Session>("Fixtures", Version::new(1, 0));
    engine.register_module(ljos::module!("Fixtures" 1.0 => [Browser]));
}

/// What a scenario reads in the log: when a window's `Document` is made
/// and when it goes.
pub(super) fn log_documents(engine: &Engine) {
    engine.on_scoped_singleton(|e, s| {
        if s.is::<Document>() {
            let said = match s.event {
                ScopedEvent::Made => "Document made",
                ScopedEvent::Dropped => "Document dropped",
            };
            e.log(LogLevel::Info, "", said);
        }
    });
}
