//! `gl_painter`: the platform has EGL, and the shell draws with GL there
//! (src/gl.rs). The same condition picks the GL crates in Cargo.toml.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(gl_painter)");
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    let unix = var("CARGO_CFG_TARGET_FAMILY")
        .split(',')
        .any(|f| f == "unix");
    if unix && var("CARGO_CFG_TARGET_VENDOR") != "apple" && var("CARGO_CFG_TARGET_OS") != "android"
    {
        println!("cargo::rustc-cfg=gl_painter");
    }
}
