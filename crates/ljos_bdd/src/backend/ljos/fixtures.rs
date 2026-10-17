//! `import Fixtures`: native types an application would register, for
//! scenarios about what the engine does with such types. No scenario that
//! does not import the module sees them.

use ljos_engine::{Engine, LogLevel, QmlObject, ScopedEvent, Version, qml_singleton};

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

pub(super) fn register(engine: &Engine) {
    engine.register_singleton::<Document>("Fixtures", Version::new(1, 0));
    engine.register_singleton::<Session>("Fixtures", Version::new(1, 0));
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
