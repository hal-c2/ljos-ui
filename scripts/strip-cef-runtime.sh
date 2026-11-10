#!/usr/bin/env bash
# strip-cef-runtime.sh DIR… strips the shared libraries of CEF's runtime that the cef crate's
# build script copied into DIR (a cargo profile directory, as target/debug).
#
# Why: on Linux CEF's minimal distribution ships them unstripped. libcef.so (CEF 156) is 1.48 GB,
# about 1 GB of it DWARF; with --strip-unneeded it is 278 MB. Every build directory
# that links CEF carries a copy, and the page cache and the disk pay for all of it. The original
# stays untouched in CEF_PATH, where the copy came from, with the same build id, so a CEF crash
# can still be symbolised (gdb: `symbol-file $CEF_PATH/<version>/cef_linux_x86_64/libcef.so`).
#
# Run it after every cargo build that may have copied the runtime: the build script copies it
# again, unstripped, whenever it reruns. A library already stripped is left alone, so a build
# that copied nothing costs a readelf. Each one is stripped into a file beside it and renamed
# over it, so an interrupted strip never leaves a half-written library, and the rename replaces
# the directory entry, never the file CEF_PATH holds. Linux only: macOS and Windows ship CEF's
# symbols separately, and on macOS the framework isn't copied here at all.
set -euo pipefail
[ "$(uname -s)" = Linux ] || exit 0
if ! command -v strip > /dev/null || ! command -v readelf > /dev/null; then
  echo "strip-cef-runtime: no strip or readelf (binutils); CEF's runtime stays unstripped" >&2
  exit 0
fi
tmp=
trap '[ -z "$tmp" ] || rm -f "$tmp"' EXIT
for dir; do
  # libcef.so is nearly all of it; SwiftShader (Vulkan on the processor, CEF's fallback when
  # there is no GPU) is 15.1 MB, 4.0 MB stripped; the Vulkan loader is 1.6 MB, 0.7 MB stripped.
  # chrome-sandbox (32 KB) is left as it ships.
  for name in libcef.so libvk_swiftshader.so libvulkan.so.1; do
    lib="$dir/$name"
    [ -f "$lib" ] || continue
    # Captured, not piped into grep -q: with pipefail, readelf killed by the closed pipe would
    # read as no match.
    sections=$(readelf -S -W "$lib")
    grep -qE ' \.(symtab|debug_[a-z_]+) ' <<< "$sections" || continue
    tmp=$(mktemp "$dir/.$name.XXXXXX")
    strip --strip-unneeded -o "$tmp" "$lib"
    chmod --reference="$lib" "$tmp"
    mv -f "$tmp" "$lib"
    tmp=
  done
done
