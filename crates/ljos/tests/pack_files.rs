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
