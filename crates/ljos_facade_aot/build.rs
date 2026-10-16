//! Compiles `qml/` against the native types `ljostypes.json` describes
//! into `$OUT_DIR/qml.pack`. `tests/types.rs` writes the file.

fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets it"));
    if let Err(e) = ljos::build::compile_dir_with_types(
        "qml".as_ref(),
        "ljostypes.json".as_ref(),
        &out.join("qml.pack"),
    ) {
        panic!("{e}");
    }
}
