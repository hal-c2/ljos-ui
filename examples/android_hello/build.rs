//! Compiles `qml/` into `$OUT_DIR/qml.pack`: the application links no
//! QML parser.

fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets it"));
    if let Err(e) = ljos::build::compile_dir("qml".as_ref(), |_| {}, &out.join("qml.pack")) {
        panic!("{e}");
    }
}
