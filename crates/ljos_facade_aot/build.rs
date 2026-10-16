//! Compiles `qml/` against the native module into `$OUT_DIR/qml.pack`.

fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets it"));
    if let Err(e) = ljos::build::compile_dir(
        "qml".as_ref(),
        ljos_facade_app::register,
        &out.join("qml.pack"),
    ) {
        panic!("{e}");
    }
}
