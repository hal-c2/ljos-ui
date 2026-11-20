//! The layout of the QuickJS types that cross the C boundary by value, on
//! Android.
//!
//! The bundled Android bindings in the vendored rquickjs-sys are copies of
//! its Linux files: bindgen against the NDK sysroot produces the same file
//! on `aarch64-linux-android` and `x86_64-linux-android`
//! (`scripts/android-bindings.sh check` regenerates and diffs them, #612;
//! `docs/spikes/02-rquickjs-android-bindgen.md`). These constants are that
//! generation's own sizes, so a plain
//! `cargo check -p ljos_js --tests --target <android>` fails if a file of
//! the wrong pointer width is ever copied in its place. The test also runs
//! wherever the test binary can run, which for Android means a device or
//! the emulator (M3).
#![cfg(target_os = "android")]

use ljos_js::rquickjs::qjs::{JSCFunctionListEntry, JSMemoryUsage, JSValue, size_t};

const _: () = {
    // Both Android targets Ljós builds for are LP64.
    assert!(size_of::<size_t>() == 8);
    assert!(size_of::<JSValue>() == 16);
    assert!(align_of::<JSValue>() == 8);
    assert!(size_of::<JSCFunctionListEntry>() == 32);
    assert!(align_of::<JSCFunctionListEntry>() == 8);
    assert!(size_of::<JSMemoryUsage>() == 208);
    assert!(align_of::<JSMemoryUsage>() == 8);
};

#[test]
fn quickjs_types_have_the_android_layout() {
    assert_eq!(size_of::<JSValue>(), 16);
    assert_eq!(size_of::<JSCFunctionListEntry>(), 32);
    assert_eq!(size_of::<JSMemoryUsage>(), 208);
}
