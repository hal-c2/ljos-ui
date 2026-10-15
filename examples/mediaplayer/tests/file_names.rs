//! The playlist names what it is given through `FileNameProvider`, from
//! a function and from an object it builds, as `PlaylistInfo.qml` does.

use ljos::dev::EngineOracle;
use ljos::prelude::*;
use ljos_compiler::{Project, Severity, compile_file};

fn engine(main: &str) -> (Engine, ObjectId) {
    let types = ljos::engine();
    ljos_example_mediaplayer::register(&types);
    let mut project = Project::new(EngineOracle::from_engine(&types));
    project.add("Main", main);
    let compiled = compile_file(&project, "Main").unwrap_or_else(|d| panic!("{d:?}"));
    let errors: Vec<_> = compiled
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(ToString::to_string)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let mut source = ljos::MapSource::new();
    source.insert("Main", compiled.file);
    let engine = ljos::engine();
    ljos_example_mediaplayer::register(&engine);
    engine.set_component_source(source);
    let root = engine.create_from_url("Main", None, vec![]).unwrap();
    (engine, root)
}

#[test]
fn a_script_names_a_file_by_the_end_of_its_url() {
    let (e, root) = engine(
        r#"import QtQuick
import io.qt.filenameprovider
Item {
    property string bound: FileNameProvider.getFileName("a/b.mp4")
    property string called: ""
    property string built: ""
    function call(path) { called = FileNameProvider.getFileName(path.toString()) }
    function build(path) {
        const file = { path: path, name: FileNameProvider.getFileName(path.toString()) }
        built = file.name
    }
}
"#,
    );
    e.invoke_by_name(root, "call", &[Value::from("file:///v/clip.mp4")]).unwrap();
    e.invoke_by_name(root, "build", &[Value::from("file:///v/song.mp3")]).unwrap();
    assert_eq!(e.read_by_name(root, "bound").to_js_string().as_str(), "b.mp4");
    assert_eq!(e.read_by_name(root, "called").to_js_string().as_str(), "clip.mp4");
    assert_eq!(e.read_by_name(root, "built").to_js_string().as_str(), "song.mp3");
}
