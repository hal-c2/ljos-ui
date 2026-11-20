#!/usr/bin/env bash
# android-bindings.sh [build | check | generate DIR]
#
# `build` compiles ljos_js for the two Android targets from the bindings
# bundled in the vendored rquickjs-sys, with only the NDK's compiler: no
# bindgen feature, so no libclang (#612).
#
# `check` (the default) is `build` plus the drift guard: it regenerates the
# bindings with bindgen against the NDK sysroot (ljos_js's `bindgen` feature,
# as in docs/spikes/02-rquickjs-android-bindgen.md) and diffs them against
# the bundled files. Anything but formatting and the numbering of anonymous
# types differing fails.
#
# `generate DIR` only regenerates: it writes DIR/<target>.rs for both
# targets and says whether each still agrees with the same architecture's
# Linux file. It needs no bundled Android file, so it is the first step of an
# rquickjs-sys upgrade (vendor/QUICKJS.md): install the Linux copies if they
# agree, the generated files if not, then run `check`.
#
# The regeneration never writes into the checkout: the bindings come from the
# build's OUT_DIR, in a throwaway target directory (a warm one would not run
# the build script again). Its generator is pinned: the root Cargo.lock
# (`--locked`) fixes bindgen, and the NDK's own libclang and clang stand in
# for the host's, so the output depends only on the tree and the NDK.
#
# Environment: ANDROID_NDK_HOME or ANDROID_NDK_ROOT names the NDK (required);
# ANDROID_API is the API level of the clang and sysroot (default 26, M3's
# minimum); LIBCLANG_PATH and CLANG_PATH override the NDK's libclang directory
# and clang. The Android rustup targets must be installed.
set -euo pipefail
cd "$(dirname "$0")/.."

usage() { echo "android-bindings: usage: $0 [build | check | generate DIR]" >&2; exit 2; }
mode="${1:-check}"
case "$mode" in
  build | check) [ $# -le 1 ] || usage ;;
  generate) [ $# -eq 2 ] || usage; dest="$2" ;;
  *) usage ;;
esac
targets=(aarch64-linux-android x86_64-linux-android)
api="${ANDROID_API:-26}"
bindings=vendor/rquickjs-sys/src/bindings

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

if [ "$mode" != generate ]; then
  for target in "${targets[@]}"; do
    echo "android-bindings: cargo build -p ljos_js --target $target"
    cargo build -p ljos_js --target "$target"
  done
  [ "$mode" = build ] && exit 0
fi

# bindgen needs libclang on the host; the regeneration is the only part that
# does. The NDK's (lib/ in current NDKs, lib64/ in old ones) ties its version
# to the NDK; its clang is the one bindgen asks for the include paths.
if [ -z "${LIBCLANG_PATH:-}" ]; then
  for dir in "$prebuilt/lib" "$prebuilt/lib64"; do
    if compgen -G "$dir/libclang.so*" > /dev/null; then LIBCLANG_PATH="$dir"; break; fi
  done
fi
[ -n "${LIBCLANG_PATH:-}" ] || {
  echo "android-bindings: no libclang.so in $prebuilt/lib or lib64; set LIBCLANG_PATH" >&2
  exit 1
}
export LIBCLANG_PATH CLANG_PATH="${CLANG_PATH:-$bin/clang}"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Writes bindgen's bindings for TARGET to OUT, formatted.
regenerate() {
  local target="$1" out="$2" suffix="${1//-/_}" sysroot_arg found
  # bindgen splits this with shlex, so the sysroot is single-quoted for it.
  sysroot_arg="--sysroot='${sysroot//\'/\'\\\'\'}'"
  # bindgen passes clang the unversioned triple, which NDK sysroots refuse;
  # defining the macros the headers check is what works (spike 02, trap 1).
  export "BINDGEN_EXTRA_CLANG_ARGS_$suffix=$sysroot_arg -D__ANDROID_MIN_SDK_VERSION__=$api -D__ANDROID_API__=$api"
  echo "android-bindings: regenerating $target bindings with bindgen"
  cargo check --locked -p ljos_js --features bindgen --target "$target" --target-dir "$work/regen-target"
  found=("$work/regen-target/$target"/debug/build/ljos_rquickjs_sys-*/out/bindings.rs)
  [ "${#found[@]}" -eq 1 ] && [ -f "${found[0]}" ] || {
    echo "android-bindings: expected one rquickjs-sys OUT_DIR for $target, found: ${found[*]}" >&2
    exit 1
  }
  cp "${found[0]}" "$out"
  rustfmt --edition 2021 "$out"
}

# Writes FILE to OUT formatted, with every identifier that holds an anonymous
# type's `_bindgen_ty_<n>` renamed by its order of first appearance: bindgen's
# numbering shifts with the headers around quickjs.h, while two distinct
# anonymous types keep two distinct names.
normalise() {
  cp "$1" "$2"
  rustfmt --edition 2021 "$2"
  perl -i -pe 's{\b(\w*_bindgen_ty_\d+\w*)}{
    my $id = $1; my $n = ($seen{$id} //= ++$k);
    (my $s = $id) =~ s/_bindgen_ty_\d+/_bindgen_ty_N/g; "$s\@$n"
  }ge' "$2"
}

# Writes the diff of files A and B, normalised, to $work/bindings.diff: empty
# when they are the same bindings. Called as a plain command, never as a
# condition, so that a failed step still aborts the script.
compare() {
  normalise "$1" "$work/a.rs"
  normalise "$2" "$work/b.rs"
  diff -u --label "$1" --label "$2" "$work/a.rs" "$work/b.rs" > "$work/bindings.diff" || [ $? -eq 1 ]
}

if [ "$mode" = generate ]; then
  mkdir -p "$dest"
  for target in "${targets[@]}"; do
    regenerate "$target" "$dest/$target.rs"
    linux="$bindings/${target%-linux-android}-unknown-linux-gnu.rs"
    compare "$linux" "$dest/$target.rs"
    if [ ! -s "$work/bindings.diff" ]; then
      echo "android-bindings: $dest/$target.rs agrees with $linux; bundle a copy of $linux"
    else
      echo "android-bindings: $dest/$target.rs differs from $linux; bundle the generated file:"
      cat "$work/bindings.diff"
    fi
  done
  exit 0
fi

status=0
for target in "${targets[@]}"; do
  regenerate "$target" "$work/$target.generated.rs"
  compare "$bindings/$target.rs" "$work/$target.generated.rs"
  if [ ! -s "$work/bindings.diff" ]; then
    echo "android-bindings: $target matches the bundled bindings"
  else
    echo "android-bindings: $target drifted from the bundled bindings" \
      "(regenerate with \`$0 generate DIR\`, vendor/QUICKJS.md):" >&2
    cat "$work/bindings.diff" >&2
    status=1
  fi
done
exit "$status"
