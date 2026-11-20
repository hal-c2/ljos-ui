#!/usr/bin/env bash
# android-bindings.sh [build|check]
#
# `build` compiles ljos_js for the two Android targets from the bindings
# bundled in the vendored rquickjs-sys, with only the NDK's compiler: no
# bindgen feature, so no libclang (#612). It also type-checks ljos_js's
# tests, whose `tests/android_layout.rs` pins the sizes those bindings
# promise.
#
# `check` (the default) is `build` plus the drift guard: it regenerates the
# bindings with rquickjs-sys's `update-bindings` feature (bindgen against
# the NDK sysroot, as in docs/spikes/02-rquickjs-android-bindgen.md) and
# diffs them against the bundled files. Both sides are formatted first
# (bindgen's own output is unformatted when its build finds no rustfmt) and
# the numbering of anonymous enums (`_bindgen_ty_N`, which no caller names)
# is normalised away. Anything else differing fails: on an rquickjs-sys
# upgrade, replace the bundled Android files with the regenerated ones (or
# with fresh copies of the new Linux files, if the diff says they still
# agree) in the same change.
#
# Environment: ANDROID_NDK_HOME or ANDROID_NDK_ROOT names the NDK (required);
# ANDROID_API is the API level of the clang and sysroot (default 26, M3's
# minimum); LIBCLANG_PATH names libclang for `check` (the usual places are
# searched when it is unset). The Android rustup targets must be installed.
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-check}"
case "$mode" in
  build | check) ;;
  *) echo "android-bindings: usage: $0 [build|check]" >&2; exit 2 ;;
esac
targets=(aarch64-linux-android x86_64-linux-android)
api="${ANDROID_API:-26}"

ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
[ -n "$ndk" ] && [ -d "$ndk" ] || {
  echo "android-bindings: set ANDROID_NDK_HOME (or ANDROID_NDK_ROOT) to an Android NDK" >&2
  exit 1
}
prebuilt=$(find "$ndk/toolchains/llvm/prebuilt" -mindepth 1 -maxdepth 1 -type d | head -1)
[ -n "$prebuilt" ] || { echo "android-bindings: no LLVM toolchain under $ndk/toolchains/llvm/prebuilt" >&2; exit 1; }
bin="$prebuilt/bin"
sysroot="$prebuilt/sysroot"

# Plain cargo, no cargo-ndk: it overwrites BINDGEN_EXTRA_CLANG_ARGS_<target>
# (spike 02), and the three variables per target are all it would set here.
for target in "${targets[@]}"; do
  clang="$bin/$target$api-clang"
  [ -x "$clang" ] || { echo "android-bindings: $clang not found; is this an NDK?" >&2; exit 1; }
  suffix="${target//-/_}"
  export "CC_$suffix=$clang" "AR_$suffix=$bin/llvm-ar"
  upper=$(echo "$suffix" | tr '[:lower:]' '[:upper:]')
  export "CARGO_TARGET_${upper}_LINKER=$clang"
done

for target in "${targets[@]}"; do
  echo "android-bindings: cargo build -p ljos_js --target $target"
  cargo build -p ljos_js --target "$target"
  cargo check -p ljos_js --tests --target "$target"
done
[ "$mode" = build ] && exit 0

# bindgen needs libclang on the host; the regeneration is the only part that does.
if [ -z "${LIBCLANG_PATH:-}" ]; then
  for dir in /usr/lib /usr/lib/llvm-*/lib /usr/lib/llvm/lib "$(llvm-config --libdir 2> /dev/null)"; do
    # The glob above is meant to expand here.
    # shellcheck disable=SC2086
    for d in $dir; do
      if [ -e "$d/libclang.so" ]; then LIBCLANG_PATH="$d"; break 2; fi
    done
  done
fi
[ -n "${LIBCLANG_PATH:-}" ] || {
  echo "android-bindings: no libclang found; install libclang-dev or set LIBCLANG_PATH" >&2
  exit 1
}
export LIBCLANG_PATH

work=$(mktemp -d)
sys_lock="vendor/rquickjs-sys/Cargo.lock"
had_sys_lock=0
[ -e "$sys_lock" ] && had_sys_lock=1
saved=""
# Invoked by the EXIT trap below.
# shellcheck disable=SC2329
cleanup() {
  # update-bindings writes into the vendored directory; the checkout keeps the bundled files.
  if [ -n "$saved" ] && [ -e "$work/$saved.rs" ]; then
    cp "$work/$saved.rs" "vendor/rquickjs-sys/src/bindings/$saved.rs"
  fi
  [ "$had_sys_lock" = 1 ] || rm -f "$sys_lock"
  rm -rf "$work"
}
trap cleanup EXIT

status=0
for target in "${targets[@]}"; do
  bundled="vendor/rquickjs-sys/src/bindings/$target.rs"
  saved="$target"
  cp "$bundled" "$work/$target.rs"
  suffix="${target//-/_}"
  # bindgen passes clang the unversioned triple, which NDK sysroots refuse;
  # defining the macros the headers check is what works (spike 02, trap 1).
  export "BINDGEN_EXTRA_CLANG_ARGS_$suffix=--sysroot=$sysroot -D__ANDROID_MIN_SDK_VERSION__=$api -D__ANDROID_API__=$api"
  echo "android-bindings: regenerating $target bindings with bindgen"
  # A throwaway target directory: the bundled file update-bindings writes is
  # not an input cargo watches, so in a warm target directory the build
  # script does not run again and nothing is regenerated.
  cargo build --manifest-path vendor/rquickjs-sys/Cargo.toml --target-dir "$work/regen-target" \
    --target "$target" --features update-bindings
  cp "$bundled" "$work/$target.generated.rs"
  cp "$work/$target.rs" "$bundled"
  saved=""
  rustfmt --edition 2021 "$work/$target.rs" "$work/$target.generated.rs"
  if diff -u \
    <(sed -E 's/_bindgen_ty_[0-9]+/_bindgen_ty_N/g' "$work/$target.rs") \
    <(sed -E 's/_bindgen_ty_[0-9]+/_bindgen_ty_N/g' "$work/$target.generated.rs") \
    > "$work/$target.diff"; then
    echo "android-bindings: $target matches the bundled bindings"
  else
    echo "android-bindings: $target drifted from the bundled bindings:" >&2
    cat "$work/$target.diff" >&2
    status=1
  fi
done
exit "$status"
