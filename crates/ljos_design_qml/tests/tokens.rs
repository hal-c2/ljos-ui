//! The files of `qml/Ljos/Design` are what `ljos_design::module` writes:
//! the numbers a check holds an interface to and the numbers the module
//! gives it are one table. `LJOS_DESIGN_TOKENS=write` writes them again.

use std::{fs, path::PathBuf};

use ljos_design::module::{self, Seeds};

#[test]
fn the_module_is_what_ljos_design_writes() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("qml")
        .join(module::DIRECTORY);
    let files = module::files(Seeds::Hues);
    if std::env::var_os("LJOS_DESIGN_TOKENS").is_some_and(|v| v == "write") {
        fs::create_dir_all(&dir).unwrap();
        for f in &files {
            fs::write(dir.join(&f.name), &f.text).unwrap();
        }
    }
    let mut stale = Vec::new();
    for f in &files {
        let committed = fs::read_to_string(dir.join(&f.name)).unwrap_or_default();
        if committed != f.text {
            stale.push(f.name.clone());
        }
    }
    let mut on_disk: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut written: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
    written.sort();
    assert_eq!(on_disk, written, "files ljos_design does not write");
    assert!(
        stale.is_empty(),
        "{stale:?} differ from what ljos_design writes: \
         LJOS_DESIGN_TOKENS=write cargo test -p ljos_design_qml --test tokens"
    );
}
