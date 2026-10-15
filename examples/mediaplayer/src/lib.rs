//! The Rust backend of the qtdoc **mediaplayer** demo
//! (`corpus/qtdoc/examples/demos/mediaplayer`), written against the
//! public `ljos` facade alone.
//!
//! The demo's QML uses one native type, the singleton
//! `FileNameProvider` (module `io.qt.filenameprovider`), whose
//! `getFileName(path)` names the file a path or URL ends in. Its
//! launcher also gives the window the three properties `Main.qml`
//! requires: the `source` to open (none), the `nameFilters` of the
//! open dialog and the `selectedNameFilter` among them.
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_example_mediaplayer::register(&engine);
//! let root = engine.create_from_url("MediaPlayer/Main", None, ljos_example_mediaplayer::initial_properties());
//! ```

// See `ljos_example_colorpalette`: the derive names `::ljos_engine`,
// which this facade-only crate stands in for.
extern crate self as ljos_engine;
#[doc(hidden)]
pub use ljos::ljos_engine::{
    __private, Atom, ConvertError, Cx, EngineError, ImplInfo, MetaBuilder, MetaObject, MetaRef,
    MethodDecl, Prop, PropertyFlags, QmlImplInfo, QmlList, QmlObject, QmlSingleton, QmlType,
    QmlValue, Reset, SignalDecl, SignalIndex, SingletonScope, TypeTag, Value,
};

use ljos::Engine;
use ljos::ljos_engine::{Version, qml_impl, qml_singleton};

/// The containers the player reads, in the dialog's words. The first
/// is the one selected, as the demo prefers MPEG-4.
const NAME_FILTERS: [&str; 7] = [
    "MPEG-4 (*.mp4 *.m4v *.m4a *.mov)",
    "Matroska (*.mkv *.webm)",
    "AVI (*.avi)",
    "MPEG transport stream (*.ts *.m2ts)",
    "Ogg (*.ogg *.oga *.opus)",
    "FLAC (*.flac)",
    "MP3 (*.mp3)",
];

/// `FileNameProvider`: what the playlist calls a file.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "io.qt.filenameprovider", name = "FileNameProvider")]
#[qml_singleton]
pub struct FileNameProvider {}

#[qml_impl]
impl FileNameProvider {
    /// The last part of a path or URL; a path that ends in `/` has none.
    #[qml(invokable, name = "getFileName")]
    fn get_file_name(_cx: &Cx, path: Atom) -> Atom {
        Atom::from(file_name(&path))
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or_default()
}

/// Registers `FileNameProvider` on `engine`.
pub fn register(engine: &Engine) {
    engine.register_singleton::<FileNameProvider>("io.qt.filenameprovider", Version::new(1, 0));
}

/// What the launcher gives the window it opens.
pub fn initial_properties() -> Vec<(String, Value)> {
    let filters = NAME_FILTERS.iter().map(|f| Value::from(*f)).collect();
    vec![
        ("source".to_owned(), Value::Url(Atom::EMPTY)),
        ("nameFilters".to_owned(), Value::list(filters)),
        ("selectedNameFilter".to_owned(), Value::Int(0)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_named_by_the_end_of_its_path() {
        assert_eq!(file_name("file:///home/user/Videos/clip.mp4"), "clip.mp4");
        assert_eq!(file_name("clip.mp4"), "clip.mp4");
        assert_eq!(file_name("/home/user/"), "");
    }

    #[test]
    fn the_singleton_is_registered_in_its_module() {
        let engine = ljos::engine();
        register(&engine);
        assert!(engine.has_module("io.qt.filenameprovider"));
        assert!(
            engine
                .lookup_type("io.qt.filenameprovider", "FileNameProvider")
                .is_some()
        );
    }
}
