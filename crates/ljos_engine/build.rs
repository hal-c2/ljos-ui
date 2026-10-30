//! Names the targets whose turn watchdog sleeps on kernel timers
//! (`src/turns/timer.rs`), so the code says `cfg(kernel_timer)` instead of
//! repeating the list of operating systems at every use.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(kernel_timer)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(
        os.as_str(),
        "linux" | "android" | "macos" | "ios" | "windows"
    ) {
        println!("cargo::rustc-cfg=kernel_timer");
    }
}
