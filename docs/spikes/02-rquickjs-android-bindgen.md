# Spike 02: rquickjs 0.14 on Android (bindgen)

Date: 2026-10-10. Milestone: M0 (prerequisite for M3, research 03 open
problem 1). Crate under test: `ljos_js` (rquickjs 0.14.0, rquickjs-sys 0.14.0,
quickjs-ng 0.16.2).

## Result

**rquickjs 0.14 builds and runs on Android, on both aarch64 and x86_64.**
Measured on 2026-10-10 with NDK r30 (30.0.16248370), API level 24, Rust
1.98.1, host libclang 22, on the `android-36` google_apis x86_64 emulator.
A scratch binary (rquickjs with `std` + `bindgen`) evaluated JS, round-tripped
JSON and called a JS function from Rust:

| Run | JS call from Rust | `for` loop of 10^6 | Binary (release, unstripped) |
|---|---|---|---|
| host x86_64 Linux | 70 ns | 29.8 ms | — |
| emulator, `x86_64-linux-android` | 85 ns | 27.7 ms | 1.80 MB |
| emulator, `aarch64-linux-android` (run through the image's ARM translation) | 303 ns | 96.6 ms | 1.83 MB |

The aarch64 numbers come from binary translation and say nothing about a
phone; they only show that the binary loads and runs. Real ARM numbers need a
device (M3).

**The generated bindings equal the bundled Linux ones.** For both aarch64 and
x86_64, the `bindings.rs` bindgen produced against the NDK sysroot is identical
to rquickjs-sys's `*-unknown-linux-gnu.rs`, apart from the numbering of
anonymous enums (`_bindgen_ty_1` vs `_bindgen_ty_3`), which no caller names.
So Option B below (copy the Linux files) is safe for 0.14.0. Re-check on every
rquickjs-sys upgrade.

**Two traps found:**
1. `bindgen` passes clang the unversioned Rust triple (`aarch64-linux-android`),
   and NDK r30's `sys/cdefs.h` stops with `Unversioned target triples are not
   supported!`. Adding `--target=aarch64-linux-android24` to
   `BINDGEN_EXTRA_CLANG_ARGS_*` does not help because bindgen appends its own
   `--target` after it. What works is defining the macro the header checks:
   `-D__ANDROID_MIN_SDK_VERSION__=24 -D__ANDROID_API__=24`.
2. **cargo-ndk 4.1.2 overwrites `BINDGEN_EXTRA_CLANG_ARGS_<target>`**, so the
   fix above is lost under `cargo ndk`. Either set the variables by hand and use
   plain `cargo build --target` (below), or use Option B and skip bindgen.

`quickjs.c` compiles with the NDK clang without warnings that stop the build.

## Reproduce

```
NDK=$HOME/Android/Sdk/ndk/30.0.16248370
BIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin
SYSROOT=$NDK/toolchains/llvm/prebuilt/linux-x86_64/sysroot
export CC_aarch64_linux_android=$BIN/aarch64-linux-android24-clang
export AR_aarch64_linux_android=$BIN/llvm-ar
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$BIN/aarch64-linux-android24-clang
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$SYSROOT -D__ANDROID_MIN_SDK_VERSION__=24 -D__ANDROID_API__=24"
cargo build --release --target aarch64-linux-android --features bindgen
adb push target/aarch64-linux-android/release/<bin> /data/local/tmp/ && adb shell /data/local/tmp/<bin>
```
(Same for `x86_64` with the names changed.) Outside the repo, mise falls back
to the system `rustc`, which has no Android targets; run cargo from the repo or
pin the toolchain with `rust-toolchain.toml`.

## Recommendation for M3

Use **Option B**: vendor or upstream the Linux bindings as
`aarch64-linux-android.rs` / `x86_64-linux-android.rs`. Then no build needs
libclang, cargo-ndk works unchanged, and CI needs only the NDK for the C
compiler. Keep a CI job that runs Option A on each rquickjs-sys upgrade and
diffs the result (ignoring `_bindgen_ty_N`), plus a layout test on the emulator.
Prefer sending the files upstream; fall back to `vendor/rquickjs-sys` with a
`[patch.crates-io]` entry. `armv7` (if 32-bit is ever wanted) must be generated,
not copied.

## Earlier analysis (before the NDK was installed)

## Why the build fails without help

`rquickjs-sys/build.rs` compiles `quickjs.c` with `cc` and, without the
`bindgen` feature, `include!`s `src/bindings/<TARGET>.rs`. The shipped list is:
aarch64/x86_64/i686/armv7/riscv64/loongarch64/powerpc64 for linux gnu and musl,
macOS (aarch64, x86_64), Windows (msvc and gnu), `wasm32-unknown-unknown`,
`wasm32-wasip1` and `wasip2`. It contains **no Android and no iOS file**. Without
the file the build script only prints "try the `bindgen` feature" and the
crate then fails on a missing `include!`.

## Option A: generate bindings in CI with the NDK (recommended first)

No vendoring, nothing to maintain; costs an NDK in CI.

1. Install the NDK, r27 or newer (r28 produces 16 KB page-aligned binaries by
   default, which Google Play requires for new 64-bit apps targeting Android
   15; with r27 add `-C link-arg=-Wl,-z,max-page-size=16384`).
   `sdkmanager "ndk;28.0.13004108"` or the `nttld/setup-ndk` GitHub action, and
   `export ANDROID_NDK_HOME=...`.
2. `rustup target add aarch64-linux-android` (and `x86_64-linux-android` for the
   emulator, `armv7-linux-androideabi` if 32-bit is wanted).
3. Point cc-rs, bindgen and the linker at the NDK clang:
   ```
   NDK_BIN=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin
   export CC_aarch64_linux_android=$NDK_BIN/aarch64-linux-android24-clang
   export AR_aarch64_linux_android=$NDK_BIN/llvm-ar
   export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$NDK_BIN/aarch64-linux-android24-clang
   export LIBCLANG_PATH=/usr/lib                      # host libclang is fine
   export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot --target=aarch64-linux-android24"
   ```
4. `cargo build -p ljos_js --target aarch64-linux-android --features bindgen`.
   `cargo-ndk` (`cargo ndk -t arm64-v8a build ...`) sets items 3 for you.

Caveats:
- rquickjs-sys warns that if the `macro` feature is on while cross-compiling,
  the host build of `rquickjs-sys` also runs bindgen with the NDK libclang and
  gets wrong bindings. `ljos_js` does not enable `macro`; keep it that way, and
  never enable `rquickjs/macro` in any crate that is built for Android.
- `bindgen` 0.72 with libclang 22 is untested here; if it rejects the version,
  use the NDK's own `libclang.so` via `LIBCLANG_PATH`.
- Whether `quickjs.c` compiles with the NDK has not been tried. quickjs-ng
  lists Android as a supported platform and `cutils.h` has `__ANDROID__`
  branches, so we expect it to.

## Option B: check in the bindings (fallback, also the upstream fix)

The bundled files are plain bindgen output and are identical for targets with
the same C ABI. Verified locally:

- `aarch64-unknown-linux-gnu.rs` and `aarch64-unknown-linux-musl.rs` are
  byte-identical, and so are the two `x86_64` linux files.
- `aarch64-apple-darwin.rs` differs from the linux file only in the name of the
  `size_t` typedef (`__darwin_size_t`) and the numbering of anonymous enums.

Android (bionic) on aarch64 and x86_64 is LP64 little-endian with the same
`size_t = unsigned long`, so the linux file should be valid unchanged. The
expectation to confirm on a machine with the NDK: generate once with Option A
plus rquickjs-sys's `update-bindings` feature, then compare with the linux file.
If equal, then either
1. send upstream a PR that adds `aarch64-linux-android.rs`,
   `x86_64-linux-android.rs` (copies of the linux files), `armv7-linux-androideabi.rs`
   (from `armv7-unknown-linux-gnueabihf`, but note that file differs from the
   `gnueabi` one by one extra struct, so regenerate it rather than copy) and
   `aarch64-apple-ios.rs` (from the darwin file); or
2. vendor `rquickjs-sys` under `vendor/` with those files and add
   `[patch.crates-io] rquickjs-sys = { path = "vendor/rquickjs-sys" }` to the
   root `Cargo.toml` (as already done for `oxc_parser`). That is a root
   manifest change, so it was not made by this task.

A layout test (`size_of::<JSRuntime>` is opaque, but `JSValue`, `JSCFunctionListEntry`,
`JSMemoryUsage` are not) in CI on the device or emulator guards against a wrong
copied file.

## iOS (same problem, noted for M4)

`aarch64-apple-ios` has no bundled file either. Option A needs Xcode's SDK
(macOS runner only); Option B is the darwin file with `target_os` unchanged.
