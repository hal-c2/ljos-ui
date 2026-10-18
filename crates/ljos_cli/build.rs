//! Link options of the `ljos` binary.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    // A position-independent binary is relocated as it loads, and glibc's
    // loader reads one 24-byte entry per pointer: 140 000 of them, 3.3 MB
    // of the file, read and walked before `main`. Packed (DT_RELR), a run of
    // pointers takes one word. The binary then needs glibc 2.36 (2022),
    // which the loader checks by version and refuses with a message, so
    // only the shipped `dist` build pays that; a debug or release build
    // still runs on an older system.
    if os == "linux" && env == "gnu" && is_dist_profile() {
        println!("cargo:rustc-link-arg-bins=-Wl,-z,pack-relative-relocs");
    }
    // The functions apps run, laid out together (`mise run bench:hot`
    // writes the list): left in crate order, the 10 MB of code an app runs
    // is spread over nearly all of the binary's 50 MB, and every page it
    // touches stays resident. A name no longer in the binary is passed
    // over, so the list only goes stale, never wrong.
    let hot = Path::new(env!("CARGO_MANIFEST_DIR")).join("link/hot.txt");
    println!("cargo:rerun-if-changed={}", hot.display());
    if os == "linux" && is_dist_profile() && hot.exists() {
        println!(
            "cargo:rustc-link-arg-bins=-Wl,--symbol-ordering-file={}",
            hot.display()
        );
        println!("cargo:rustc-link-arg-bins=-Wl,--no-warn-symbol-ordering");
    }
}

/// Whether this is the `dist` profile. Cargo tells a build script only the
/// profile a custom one inherits (`release`), but builds each profile in a
/// directory of its own name: `<target>[/<triple>]/dist/build/<pkg>/out`.
fn is_dist_profile() -> bool {
    let Ok(out) = std::env::var("OUT_DIR") else {
        return false;
    };
    Path::new(&out)
        .ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "build"))
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .is_some_and(|profile| profile == "dist")
}
