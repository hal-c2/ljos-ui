//! The files a pack carries besides its QML when the build packs several
//! directories (`build::Options::pack`): a module's `qrc:` address names
//! the files of its own directory alone (features/tooling/pack-files.feature
//! packs one).

fn write(root: &std::path::Path, name: &str, bytes: &[u8]) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_modules_address_names_the_files_of_its_own_directory() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(a.path(), "Shapes/ring.png", b"not the module's");
    write(b.path(), "Shapes/qmldir", b"module Shapes\n");
    write(b.path(), "Shapes/dot.png", b"the module's");

    let pack = ljos::build::Options::new()
        .without_sources()
        .pack(&ljos::MapSource::new(), &[a.path(), b.path()])
        .unwrap();
    let source = ljos::MapSource::from_pack(&pack).unwrap();

    let carried: Vec<&str> = source.resources().map(|(key, _)| key).collect();
    assert_eq!(carried, ["Shapes/dot.png", "Shapes/ring.png"]);
    let named: Vec<(&str, &str)> = source.resource_addresses().collect();
    assert_eq!(named, [("qrc:/qt/qml/Shapes/dot.png", "Shapes/dot.png")]);
}

/// What a pack of `dirs` carries, by key, and its addresses.
fn packed(
    options: ljos::build::Options,
    dirs: &[&std::path::Path],
) -> (Vec<(String, Vec<u8>)>, Vec<(String, String)>) {
    let pack = options
        .without_sources()
        .pack(&ljos::MapSource::new(), dirs)
        .unwrap();
    let source = ljos::MapSource::from_pack(&pack).unwrap();
    let carried = source
        .resources()
        .map(|(key, bytes)| (key.to_owned(), bytes.to_vec()))
        .collect();
    let named = source
        .resource_addresses()
        .map(|(address, key)| (address.to_owned(), key.to_owned()))
        .collect();
    (carried, named)
}

/// The bytes the pack serves at `address`.
fn at<'a>(
    (carried, named): &'a (Vec<(String, Vec<u8>)>, Vec<(String, String)>),
    address: &str,
) -> Option<&'a [u8]> {
    let (_, key) = named.iter().find(|(a, _)| a == address)?;
    carried
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, bytes)| bytes.as_slice())
}

#[test]
fn a_modules_file_at_a_path_another_directory_also_has_is_still_at_its_address() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(a.path(), "Shapes/dot.png", b"not the module's");
    write(b.path(), "Shapes/qmldir", b"module Shapes\n");
    write(b.path(), "Shapes/dot.png", b"the module's");

    let pack = packed(ljos::build::Options::new(), &[a.path(), b.path()]);

    assert_eq!(
        at(&pack, "qrc:/qt/qml/Shapes/dot.png"),
        Some(&b"the module's"[..]),
        "{pack:?}"
    );
    // The path itself is still the first directory's file.
    let plain = pack.0.iter().find(|(k, _)| k == "Shapes/dot.png");
    assert_eq!(
        plain.map(|(_, b)| b.as_slice()),
        Some(&b"not the module's"[..])
    );
}

#[test]
fn a_file_a_modules_build_keeps_at_a_path_another_directory_also_has_is_its_own() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(a.path(), "Shapes/t.csv", b"not the module's");
    write(b.path(), "Shapes/qmldir", b"module Shapes\n");
    write(
        b.path(),
        "Shapes/CMakeLists.txt",
        b"qt_add_qml_module(shapes URI Shapes RESOURCES t.csv)\n",
    );
    write(b.path(), "Shapes/t.csv", b"the module's");

    // Carrying no other files, only the module's is carried.
    let pack = packed(
        ljos::build::Options::new().without_files(),
        &[a.path(), b.path()],
    );
    assert_eq!(
        at(&pack, "qrc:/qt/qml/Shapes/t.csv"),
        Some(&b"the module's"[..]),
        "{pack:?}"
    );

    // Carrying csv files, the first directory's file has the path, and
    // the address still names the module's.
    let pack = packed(
        ljos::build::Options::new().with_files(["csv"]),
        &[a.path(), b.path()],
    );
    assert_eq!(
        at(&pack, "qrc:/qt/qml/Shapes/t.csv"),
        Some(&b"the module's"[..]),
        "{pack:?}"
    );
    let plain = pack.0.iter().find(|(k, _)| k == "Shapes/t.csv");
    assert_eq!(
        plain.map(|(_, b)| b.as_slice()),
        Some(&b"not the module's"[..])
    );
}

const SHAPES: &[u8] = b"module Shapes\nRing 1.0 Ring.qml\n";
const RING: &[u8] = b"import QtQuick\nItem {}\n";

#[test]
fn a_file_the_build_of_a_module_no_import_sees_keeps_has_no_address() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "Shapes/qmldir", SHAPES);
    write(root.path(), "Shapes/Ring.qml", RING);
    write(
        root.path(),
        "old/CMakeLists.txt",
        b"qt_add_qml_module(old_shapes URI Shapes RESOURCES icons/old.png)\n",
    );
    write(root.path(), "old/icons/old.png", b"the stand-in's");

    let pack = packed(ljos::build::Options::new(), &[root.path()]);

    assert_eq!(
        at(&pack, "qrc:/qt/qml/Shapes/icons/old.png"),
        None,
        "{pack:?}"
    );
    assert!(pack.0.iter().any(|(k, _)| k == "old/icons/old.png"));
}

#[test]
fn an_import_chooses_a_modules_directory_over_all_the_packed_ones() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(a.path(), "old/qmldir", SHAPES);
    write(a.path(), "old/Ring.qml", RING);
    write(
        a.path(),
        "old/CMakeLists.txt",
        b"qt_add_qml_module(old_shapes URI Shapes RESOURCES old.csv)\n",
    );
    write(a.path(), "old/old.csv", b"the stand-in's");
    write(a.path(), "old/dot.png", b"the stand-in's");
    write(b.path(), "Shapes/qmldir", SHAPES);
    write(b.path(), "Shapes/Ring.qml", RING);
    write(b.path(), "Shapes/dot.png", b"the module's");

    let pack = packed(ljos::build::Options::new(), &[a.path(), b.path()]);

    assert_eq!(
        at(&pack, "qrc:/qt/qml/Shapes/dot.png"),
        Some(&b"the module's"[..]),
        "{pack:?}"
    );
    assert_eq!(at(&pack, "qrc:/qt/qml/Shapes/old.csv"), None, "{pack:?}");
    assert!(pack.0.iter().any(|(k, _)| k == "old/old.csv"));
}

#[test]
fn a_file_carried_for_its_address_alone_is_registered_there_alone() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    write(a.path(), "Shapes/dot.png", b"not the module's");
    write(b.path(), "Shapes/qmldir", b"module Shapes\n");
    write(b.path(), "Shapes/dot.png", b"the module's");
    let pack = ljos::build::Options::new()
        .without_sources()
        .pack(&ljos::MapSource::new(), &[a.path(), b.path()])
        .unwrap();
    let engine = ljos::engine();

    ljos::register_pack_files(&engine, &ljos::MapSource::from_pack(&pack).unwrap());

    let platform = ljos_platform::Platform::of(&engine);
    let loaded = |url: &str| platform.resource(url).map(|bytes| bytes.to_vec());
    let address = "qrc:/qt/qml/Shapes/dot.png";
    assert_eq!(loaded(address), Some(b"the module's".to_vec()));
    assert_eq!(
        loaded("file:///Shapes/dot.png"),
        Some(b"not the module's".to_vec())
    );
    assert_eq!(loaded(&format!("file:///{address}")), None);
}
