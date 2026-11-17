//! A pack built for a program its users change carries the sources its
//! files were compiled from (`build::Options::with_sources`), and runs as
//! one without them does (ARCHITECTURE.md §12.1).

use ljos_compiler::sources::{PackSources, SourcesError};

const MAIN: &str = "import QtQuick\nItem { property int seen: card.width; Card { id: card } }\n";
const CARD: &str = "import QtQuick\nRectangle { width: 10 }\n";

fn qml_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), MAIN).unwrap();
    std::fs::write(dir.path().join("Card.qml"), CARD).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "not a source").unwrap();
    dir
}

fn pack(options: &ljos::build::Options) -> Vec<u8> {
    let dir = qml_dir();
    let out = tempfile::tempdir().unwrap();
    let pack = out.path().join("qml.pack");
    options.compile_dirs(&[dir.path()], |_| {}, &pack).unwrap();
    std::fs::read(&pack).unwrap()
}

#[test]
fn a_pack_with_its_sources_carries_the_text_of_every_file_it_compiled() {
    let bytes = pack(&ljos::build::Options::new().with_sources());
    let source = ljos::MapSource::from_pack(&bytes).unwrap();
    let carried = PackSources::decode(source.trailer().expect("a trailer")).unwrap();
    assert_eq!(
        carried.files.get("Main.qml").map(String::as_str),
        Some(MAIN)
    );
    assert_eq!(
        carried.files.get("Card.qml").map(String::as_str),
        Some(CARD)
    );
    assert!(!carried.files.contains_key("notes.txt"));

    // It runs as the pack without them does.
    let (engine, root) = ljos::App::new("Test")
        .source(source)
        .root("Main")
        .build()
        .unwrap();
    assert_eq!(engine.read_by_name(root, "seen").to_number(), 10.0);
}

#[test]
fn a_pack_without_its_sources_carries_its_files_alone() {
    let without = pack(&ljos::build::Options::new().without_sources());
    let with = pack(&ljos::build::Options::new().with_sources());
    let source = ljos::MapSource::from_pack(&without).unwrap();
    assert!(
        source
            .trailer()
            .is_none_or(|t| matches!(PackSources::decode(t), Err(SourcesError::Missing)))
    );
    assert!(without.len() < with.len());
    // A build script whose `ljos` has no `user-qml` packs no sources
    // unless asked.
    let default = pack(&ljos::build::Options::default());
    assert_eq!(
        default,
        if cfg!(feature = "user-qml") {
            with
        } else {
            without
        }
    );
}
