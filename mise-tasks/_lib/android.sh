# shellcheck shell=bash
# What the Android tasks share, sourced after env.sh. Nothing is installed:
# a missing NDK or Rust target is an error naming what to install.

# The ABIs Ljós builds (ARCHITECTURE.md §10.3) and their Rust targets.
android_abis="arm64-v8a x86_64"
android_min_sdk=26

android_rust_target() {
  case "$1" in
    arm64-v8a | aarch64-linux-android) echo aarch64-linux-android ;;
    x86_64 | x86_64-linux-android) echo x86_64-linux-android ;;
    *) die "$(task_name): no Android ABI $1; Ljós builds $android_abis" ;;
  esac
}

# The SDK: ANDROID_HOME, ANDROID_SDK_ROOT, else where Android Studio puts it.
android_sdk() {
  local d
  for d in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "$HOME/Android/Sdk" "$HOME/Library/Android/sdk"; do
    [ -n "$d" ] && [ -d "$d" ] && { printf '%s\n' "$d"; return 0; }
  done
  return 1
}

# The NDK: ANDROID_NDK_HOME, ANDROID_NDK_ROOT, else the newest under the SDK.
android_ndk() {
  local d sdk newest=""
  for d in "${ANDROID_NDK_HOME:-}" "${ANDROID_NDK_ROOT:-}"; do
    [ -n "$d" ] && [ -d "$d/toolchains/llvm" ] && { printf '%s\n' "$d"; return 0; }
  done
  if sdk="$(android_sdk)" && [ -d "$sdk/ndk" ]; then
    for d in "$sdk"/ndk/*; do
      [ -d "$d/toolchains/llvm" ] && newest="$d"
    done
  fi
  [ -n "$newest" ] && { printf '%s\n' "$newest"; return 0; }
  die "$(task_name) needs the Android NDK (r27 or newer): set ANDROID_NDK_HOME, or install it with the SDK manager (ndk;30.0.16248370)"
}

# android_ndk_prebuilt: the NDK's LLVM toolchain for this machine.
android_ndk_prebuilt() {
  local ndk host
  ndk="$(android_ndk)"
  case "$(uname -s)" in
    Linux) host=linux-x86_64 ;;
    Darwin) host=darwin-x86_64 ;;
    *) host=windows-x86_64 ;;
  esac
  printf '%s\n' "$ndk/toolchains/llvm/prebuilt/$host"
}

# android_cross_env <rust target>: the C compiler, archiver and linker of the
# NDK for cargo and cc-rs, at the minimum API level, the variables cargo-ndk
# sets, so that plain `cargo build --target` works the same.
android_cross_env() {
  local triple="$1" bin var
  bin="$(android_ndk_prebuilt)/bin"
  [ -x "$bin/$triple$android_min_sdk-clang" ] ||
    die "$(task_name): the NDK at $(android_ndk) has no $triple$android_min_sdk-clang"
  var="$(printf '%s' "$triple" | tr '-' '_')"
  export "CC_$var=$bin/$triple$android_min_sdk-clang"
  export "CXX_$var=$bin/$triple$android_min_sdk-clang++"
  export "AR_$var=$bin/llvm-ar"
  var="$(printf '%s' "$triple" | tr '[:lower:]-' '[:upper:]_')"
  export "CARGO_TARGET_${var}_LINKER=$bin/$triple$android_min_sdk-clang"
  export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(android_ndk)}"
}

# android_adb: the SDK's adb, else the one on PATH.
android_adb() {
  local sdk
  if sdk="$(android_sdk)" && [ -x "$sdk/platform-tools/adb" ]; then
    printf '%s\n' "$sdk/platform-tools/adb"
  elif command -v adb > /dev/null; then
    command -v adb
  else
    die "$(task_name) needs adb: install the SDK's platform-tools with the SDK manager (platform-tools)"
  fi
}
