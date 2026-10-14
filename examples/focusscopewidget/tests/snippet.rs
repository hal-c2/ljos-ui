//! The focus snippet itself
//! (`corpus/qtdoc/doc/src/snippets/declarative/focus`), compiled
//! against an Engine this crate's `MyFocusScopeWidget` is registered
//! on, and created.

use std::path::PathBuf;

use ljos::dev::EngineOracle;
use ljos::prelude::*;
use ljos_compiler::{Project, Severity, compile_file};

fn snippet_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/qtdoc/doc/src/snippets/declarative/focus")
}

fn corpus_present(test: &str) -> bool {
    let dir = snippet_dir();
    if dir.is_dir() {
        return true;
    }
    use std::io::Write as _;
    let _ = writeln!(
        std::io::stderr(),
        "SKIPPED {test}: no corpus at {} (`git submodule update --init corpus/qtdoc`)",
        dir.display()
    );
    false
}

fn engine() -> Engine {
    let engine = ljos::engine();
    ljos_example_focusscopewidget::register(&engine);
    let dir = snippet_dir();
    let project = Project::load(&dir, EngineOracle::from_engine(&engine))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut source = ljos::MapSource::new();
    for key in project.files().map(str::to_owned).collect::<Vec<_>>() {
        // `focusColumn.qml` is another snippet's; only the entry
        // is this crate's concern.
        if key != "focusscopewidget" {
            continue;
        }
        let compiled = compile_file(&project, &key).unwrap_or_else(|d| panic!("{key}: {d:?}"));
        let errors: Vec<String> = compiled
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(ToString::to_string)
            .collect();
        assert!(errors.is_empty(), "{key}: {errors:?}");
        source.insert(&key, compiled.file);
    }
    engine.set_component_source(source);
    engine
}

fn walk(e: &Engine, root: ObjectId) -> Vec<ObjectId> {
    let mut all = Vec::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        all.push(o);
        stack.extend(e.children(o));
    }
    all
}

#[test]
fn the_snippet_compiles_and_makes_two_widgets_with_the_colours_it_gave() {
    if !corpus_present("the_snippet_compiles_and_makes_two_widgets") {
        return;
    }
    let e = engine();
    let root = e
        .create_from_url("focusscopewidget", None, Vec::new())
        .unwrap_or_else(|err| panic!("snippet: {err}"));
    let colors: Vec<String> = walk(&e, root)
        .into_iter()
        .filter(|&o| e.read_by_name(o, "width").to_number() == 160.0)
        .filter(|&o| !matches!(e.read_by_name(o, "color"), Value::Undefined))
        .map(|o| e.read_by_name(o, "color").to_js_string().to_string())
        .collect();
    // The widget and its face read the same colour through the
    // alias; both widgets are there.
    assert!(
        colors.iter().any(|c| c.contains("add8e6")) && colors.iter().any(|c| c.contains("98fb98")),
        "colours: {colors:?}"
    );
}

#[test]
fn the_widget_has_its_own_implicit_size() {
    if !corpus_present("the_widget_has_its_own_implicit_size") {
        return;
    }
    let e = engine();
    let root = e
        .create_from_url("focusscopewidget", None, Vec::new())
        .expect("snippet");
    let widgets: Vec<ObjectId> = walk(&e, root)
        .into_iter()
        .filter(|&o| e.read_by_name(o, "height").to_number() == 30.0)
        .collect();
    assert!(!widgets.is_empty());
}
