//! The helper finds libcef beside itself, where an app ships the two.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let cef = std::env::var_os("CARGO_FEATURE_CEF").is_some();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if cef && os == "linux" {
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
}
