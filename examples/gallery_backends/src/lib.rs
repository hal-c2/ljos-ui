//! The native backends of qtdoc gallery examples, by example id.
//!
//! Several examples in the gallery corpus (`corpus/examples.json`)
//! register their own types from C++ when run under Qt. Those backends
//! are reimplemented here in Rust, each as an ordinary application
//! module written against the public `ljos` facade alone, from the
//! interface the example's QML uses (CLEAN_ROOM.md: the C++ is never
//! read). A host that renders gallery examples — the gallery renderer
//! today — asks [`register`] for an example's backends by its manifest
//! id and registers them on its Engine like any other module.
//!
//! A generic Qt module used by an example (for instance
//! `QtQml.XmlListModel`) is not an example backend: it lives in its own
//! crate and appears here only as a registration.

use ljos::Engine;
use ljos::prelude::Value;

/// Registers the backends of the example `id` (a `corpus/examples.json`
/// id) on `engine`. An example without a backend registers nothing.
pub fn register(engine: &Engine, id: &str) {
    if let Some(register) = backends(id) {
        register(engine);
    }
}

/// Whether the example `id` registers types of its own: one without
/// compiles against the table of types alone, as any program does.
pub fn has_backends(id: &str) -> bool {
    backends(id).is_some()
}

fn backends(id: &str) -> Option<fn(&Engine)> {
    Some(match id {
        // The ColorPalette client's own types (RestService and its
        // resources), in the module its QML files are types of.
        "colorpaletteclient" => ljos_example_colorpalette::register,
        // No types of its own: the generic `QtQml.XmlListModel` module
        // its feed is read with.
        "rssnews" => ljos_xml::register,
        // The generic `Qt.labs.folderlistmodel` module it lists pictures with.
        "photosurface" => ljos_folderlistmodel::register,
        // `BackEnd`, answering as a device with none of the Android
        // services the demo is about.
        "qtjennydemo" => ljos_example_qtjennydemo::register,
        // The snippet's widget, which its qmldir names and the
        // corpus does not carry.
        "snippet-focus-scope-widget" => ljos_example_focusscopewidget::register,
        // `FileNameProvider`; QtMultimedia itself is a standard module.
        "mediaplayer" => ljos_example_mediaplayer::register,
        _ => return None,
    })
}

/// The properties the launcher of the example `id` gives its root
/// object (in Qt, `setInitialProperties`): those a root `required`
/// property needs. An example whose launcher gives none has none.
pub fn initial_properties(id: &str) -> Vec<(String, Value)> {
    match id {
        "mediaplayer" => ljos_example_mediaplayer::initial_properties(),
        _ => Vec::new(),
    }
}

/// The example ids that have a backend, for a host that wants to say
/// which examples it can run whole.
pub fn supported() -> &'static [&'static str] {
    &[
        "colorpaletteclient",
        "rssnews",
        "qtjennydemo",
        "snippet-focus-scope-widget",
        "mediaplayer",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_supported_example_registers_its_types() {
        let engine = ljos::engine();
        register(&engine, "colorpaletteclient");
        assert!(engine.has_module("ColorPalette"));
        assert!(engine.lookup_type("ColorPalette", "RestService").is_some());

        let engine = ljos::engine();
        register(&engine, "qtjennydemo");
        assert!(engine.has_module("qtjenny_consumer"));
        assert!(engine.lookup_type("qtjenny_consumer", "BackEnd").is_some());

        let engine = ljos::engine();
        register(&engine, "snippet-focus-scope-widget");
        assert!(
            engine
                .lookup_type("QtQuick", "MyFocusScopeWidget")
                .is_some()
        );

        let engine = ljos::engine();
        register(&engine, "mediaplayer");
        assert!(
            engine
                .lookup_type("io.qt.filenameprovider", "FileNameProvider")
                .is_some()
        );
        assert_eq!(initial_properties("mediaplayer").len(), 3);
        assert!(initial_properties("clocks").is_empty());

        let engine = ljos::engine();
        register(&engine, "rssnews");
        assert!(engine.has_module("QtQml.XmlListModel"));
        assert!(
            engine
                .lookup_type("QtQml.XmlListModel", "XmlListModel")
                .is_some()
        );
    }

    #[test]
    fn an_example_without_a_backend_registers_nothing() {
        let engine = ljos::engine();
        let before = engine.type_dump();
        register(&engine, "no-such-example");
        register(&engine, "clocks");
        assert_eq!(engine.type_dump(), before);
        assert!(!supported().contains(&"no-such-example"));
    }

    #[test]
    fn every_supported_id_registers() {
        for id in supported() {
            let engine = ljos::engine();
            let before = engine.type_dump();
            register(&engine, id);
            assert_ne!(engine.type_dump(), before, "{id} registered nothing");
        }
    }
}
