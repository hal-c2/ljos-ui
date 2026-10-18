//! Link options of the `ljos` binary.

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    // A position-independent binary is relocated as it loads, and glibc's
    // loader reads one 24-byte entry per pointer: 140 000 of them, 3.3 MB
    // of the file, read and walked before `main`. Packed (DT_RELR), a run of
    // pointers takes one word. The binary then needs glibc 2.36 (2022),
    // which the loader checks by version and refuses with a message.
    if os == "linux" && env == "gnu" {
        println!("cargo:rustc-link-arg-bins=-Wl,-z,pack-relative-relocs");
    }
}
