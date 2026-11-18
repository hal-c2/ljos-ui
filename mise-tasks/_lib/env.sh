# shellcheck shell=bash
# What the tasks share. Sourced, not a task: it is not executable, so mise does
# not list it. Every environment variable named here stays an override, and
# nothing is installed unasked: what is missing is an error with the one
# command that gets it.
#
# Runs on the bash 3.2 macOS ships and on BSD tools: no `mapfile`, associative
# arrays, `${v^^}`, `globstar`, `readlink -f`, `realpath`, `sed -i`, `stat -c`,
# or an empty array under `set -u` without `${a[@]+"${a[@]}"}`.
# `mise run tasks:smoke` holds every task to that.

ljos_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

die() {
  printf '%s\n' "$*" >&2
  exit 1
}

# need <program> <how to get it>: stops the task before it starts when a
# program it runs is not on PATH.
need() {
  command -v "$1" >/dev/null 2>&1 ||
    die "${MISE_TASK_NAME:-this task} needs $1, which is not on PATH: $2"
}

# A directory with its links resolved.
physical() {
  (cd "$1" && pwd -P)
}

# A path a person typed, absolute from where they typed it: mise runs a task
# in the repository's root, not where the person was. It need not exist yet.
from_asked() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s/%s\n' "${MISE_ORIGINAL_CWD:-$PWD}" "$1" ;;
  esac
}

# CARGO_TARGET_DIR as an absolute path (a relative one is from the root, where
# the tasks build), else the checkout's target/.
target_dir() {
  case "${CARGO_TARGET_DIR:-}" in
    "") printf '%s\n' "$ljos_root/target" ;;
    /*) printf '%s\n' "$CARGO_TARGET_DIR" ;;
    *) printf '%s\n' "$ljos_root/$CARGO_TARGET_DIR" ;;
  esac
}

# Off-screen Qt: no window opens, and a platform theme set for the desktop
# (gtk3, qt6ct) would still try to reach a display and end qml6 without one.
qt_offscreen() {
  export QT_QPA_PLATFORM=offscreen
  unset QT_QPA_PLATFORMTHEME
}

need_qml6() {
  need "${QML6:-qml6}" "install Qt 6's QML runtime (Arch: pacman -S qt6-declarative; Debian, Ubuntu: apt install qml6-module-qtquick qt6-declarative-dev-tools; macOS: brew install qt), or set QML6 to Qt's bin/qml"
}

need_python3() {
  need python3 "install Python 3 (Arch: pacman -S python; Debian, Ubuntu: apt install python3; macOS: brew install python)"
}

need_curl() {
  need curl "install curl (Arch: pacman -S curl; Debian, Ubuntu: apt install curl; macOS ships it)"
}

need_corpus() {
  [ -d "$ljos_root/corpus/qtdoc/examples" ] ||
    die "${MISE_TASK_NAME:-this task} needs the qtdoc corpus, which is not checked out: git submodule update --init --depth 1 corpus/qtdoc (or mise run setup)"
}

# Qt 6's qmltestrunner: QMLTESTRUNNER, else the one in the bin directory Qt's
# own tools name, else Qt 6's usual places, else qmltestrunner6. A plain
# `qmltestrunner` on PATH is often Qt 5's, so it is taken only when it says 6.
find_qmltestrunner() {
  if [ -n "${QMLTESTRUNNER:-}" ]; then
    [ -x "$QMLTESTRUNNER" ] || die "QMLTESTRUNNER is $QMLTESTRUNNER, which is not a program"
    printf '%s\n' "$QMLTESTRUNNER"
    return 0
  fi
  local tool bins candidate
  for tool in qtpaths6 qmake6; do
    command -v "$tool" >/dev/null 2>&1 || continue
    bins="$("$tool" --query QT_INSTALL_BINS 2>/dev/null || "$tool" -query QT_INSTALL_BINS 2>/dev/null || true)"
    if [ -n "$bins" ] && [ -x "$bins/qmltestrunner" ]; then
      printf '%s\n' "$bins/qmltestrunner"
      return 0
    fi
  done
  for candidate in /usr/lib/qt6/bin/qmltestrunner /usr/lib64/qt6/bin/qmltestrunner \
    /usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner /usr/lib/aarch64-linux-gnu/qt6/bin/qmltestrunner \
    "$(command -v qmltestrunner6 2>/dev/null || true)"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  candidate="$(command -v qmltestrunner 2>/dev/null || true)"
  if [ -n "$candidate" ] && QT_QPA_PLATFORM=offscreen "$candidate" --version 2>&1 | grep -q ' 6\.'; then
    printf '%s\n' "$candidate"
    return 0
  fi
  die "${MISE_TASK_NAME:-this task} needs Qt 6's qmltestrunner, which was not found: install Qt 6's QML test tools (Arch: pacman -S qt6-declarative; Debian, Ubuntu: apt install qt6-declarative-dev-tools), or set QMLTESTRUNNER"
}

# --- The Qt CI pins ------------------------------------------------------------

# The Qt version CI builds and compares against, as the install-qt-action steps
# of .github/workflows/ci.yml write it: the one place it is written.
# LJOS_CI_WORKFLOW reads another file (the tests do).
qt_pin() {
  local ci="${LJOS_CI_WORKFLOW:-$ljos_root/.github/workflows/ci.yml}" pins
  [ -f "$ci" ] || die "no $ci, which pins the Qt CI uses"
  pins="$(awk '
    /uses:[[:space:]]*jurplel\/install-qt-action/ { step = 1; next }
    /^[[:space:]]*-[[:space:]]/ { step = 0 }
    step && /^[[:space:]]*version:/ { v = $2; gsub(/["\047]/, "", v); print v; step = 0 }
  ' "$ci" | sort -u)"
  case "$pins" in
    "") die "$ci has no install-qt-action step with a version, so there is no Qt to compare with" ;;
    *"
"*) die "$ci's install-qt-action steps pin different Qt versions ($(printf '%s' "$pins" | tr '\n' ' ')): make them one" ;;
  esac
  printf '%s\n' "$pins"
}

# What Qt 6 on this machine says it is, one "<tool> <version>" line per tool
# that answers (Qt 5's tools, often what a bare `qtpaths` is, are left out).
qt_found() {
  local tool v
  for tool in qmake6 qtpaths6 qtpaths qml6; do
    command -v "$tool" >/dev/null 2>&1 || continue
    case "$tool" in
      qmake6) v="$("$tool" -query QT_VERSION 2>/dev/null || true)" ;;
      qml6) v="$(QT_QPA_PLATFORM=offscreen "$tool" --version 2>/dev/null | sed -n 's/^Qml Runtime //p' | sed -n 1p || true)" ;;
      *) v="$("$tool" --qt-version 2>/dev/null || true)" ;;
    esac
    case "$v" in
      6.*) printf '%s %s\n' "$tool" "$v" ;;
    esac
  done
}

# The one-line way to get the pinned Qt, for this platform.
qt_pin_fix() {
  local pin="$1" host=linux arch=linux_gcc_64 dir=gcc_64
  if [ "$(uname -s)" = Darwin ]; then
    host=mac arch=clang_64 dir=macos
  fi
  printf 'install Qt %s as CI does (pip install aqtinstall && aqt install-qt %s desktop %s %s -m qtmultimedia -O ~/Qt) and put ~/Qt/%s/%s/bin first on PATH' \
    "$pin" "$host" "$pin" "$arch" "$pin" "$dir"
}

# Empty when every Qt 6 tool found is CI's minor version (or none is found);
# otherwise one line per other version, naming the tools that report it, with
# the fix.
qt_mismatch() {
  local pin want
  pin="$(qt_pin)"
  want="${pin%.*}"
  qt_found | awk -v want="$want" -v pin="$pin" -v fix="$(qt_pin_fix "$pin")" '
    $2 != want && index($2, want ".") != 1 {
      if (!($2 in tools)) order[++n] = $2
      tools[$2] = ($2 in tools) ? tools[$2] ", " $1 : $1
    }
    END {
      for (i = 1; i <= n; i++)
        printf "Qt %s (%s) is not CI'"'"'s Qt %s (.github/workflows/ci.yml pins %s), so its type table and qml6'"'"'s pictures are not what CI compares with: %s\n", order[i], tools[order[i]], want, pin, fix
    }'
}

# --- The Rust mise.toml pins ---------------------------------------------------

# A field of mise.toml's `rust = { … }` line: version, components or targets
# (the last two as a space-separated list).
rust_pin() {
  sed -n "s/^rust *= *{.*$1 *= *\"\\([^\"]*\\)\".*/\\1/p" "$ljos_root/mise.toml" | tr ',' ' '
}

no_rustup() {
  echo "no rustup on PATH: mise installs Ljós's Rust (mise.toml) through rustup. Install it (https://rustup.rs: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh), open a new shell, then run mise run setup again"
}

# Empty when the rustc tasks build with is mise.toml's; otherwise why not,
# with the fix.
wrong_rustc() {
  local pin have at
  pin="$(rust_pin version)"
  have="$(rustc --version 2>/dev/null || true)"
  case "$have" in
    "rustc $pin" | "rustc $pin "*) return 0 ;;
  esac
  at="$(command -v rustc || true)"
  if [ -z "$at" ]; then
    echo "no rustc is on PATH: run mise run setup, which installs Rust $pin (mise.toml) with rustup"
    return 0
  fi
  echo "the rustc tasks build with ($at) is ${have:-not working}, not mise.toml's $pin: run mise run setup; if that does not change it, rustup's proxies (~/.cargo/bin) must come before $(dirname "$at") on PATH"
}

# The parts of mise.toml's `components` or `targets` rustup does not list as
# installed. Fails when rustup cannot list them (no toolchain yet).
missing_rust_parts() {
  local installed part missing=""
  if [ "$1" = components ]; then
    installed="$(rustup component list --installed 2>/dev/null)" || return 1
  else
    installed="$(rustup target list --installed 2>/dev/null)" || return 1
  fi
  [ -n "$installed" ] || return 1
  for part in $(rust_pin "$1"); do
    case "
$installed
" in
      *"
$part
"* | *"
$part-"*) ;;
      *) missing="$missing $part" ;;
    esac
  done
  printf '%s\n' "${missing# }"
}

# at-spi2-registryd is not on PATH: it lives in a libexec directory.
find_atspi_registry() {
  local bin
  for bin in /usr/lib/at-spi2-registryd /usr/libexec/at-spi2-registryd /usr/lib/at-spi2-core/at-spi2-registryd /usr/lib/*/at-spi2-registryd; do
    if [ -x "$bin" ]; then
      printf '%s\n' "$bin"
      return 0
    fi
  done
  return 1
}

# Says which Rust a build uses, and stops one that is not mise.toml's: it
# lacks the components and target CI builds with, and its lints differ.
need_pinned_rust() {
  local why
  cargo --version
  why="$(wrong_rustc)"
  [ -z "$why" ] || die "${MISE_TASK_NAME:-this task}: $why"
}

# The Vulkan loader starts every driver whose manifest it finds, NVIDIA's too
# when only its user-space package is installed (no card, or its kernel driver
# not loaded), and a test that makes a wgpu instance can crash in it. An app's
# shell leaves that driver out (crates/ljos_shell/src/drivers.rs); this does
# the same for a task's tests, through the variable the loader reads from
# 1.3.234 on (an older one ignores it). Drivers the user chose stand.
# leave_out_absent_nvidia [kernel driver path [manifest dir...]]: the
# arguments are for tasks:test.
leave_out_absent_nvidia() {
  local loaded="${1:-/proc/driver/nvidia}" v d
  [ "$(uname -s)" = Linux ] || return 0
  [ ! -e "$loaded" ] || return 0
  for v in VK_DRIVER_FILES VK_ICD_FILENAMES VK_ADD_DRIVER_FILES \
    VK_LOADER_DRIVERS_SELECT VK_LOADER_DRIVERS_DISABLE; do
    [ -z "${!v+set}" ] || return 0
  done
  if [ $# -gt 1 ]; then
    shift
  else
    set -- /etc/vulkan/icd.d /usr/local/share/vulkan/icd.d /usr/share/vulkan/icd.d
  fi
  for d in "$@"; do
    if ls "$d"/nvidia*.json >/dev/null 2>&1; then
      export VK_LOADER_DRIVERS_DISABLE='*nvidia*'
      return 0
    fi
  done
}

need_valgrind() {
  need valgrind "install Valgrind (Arch: pacman -S valgrind; Debian, Ubuntu: apt install valgrind; not on macOS)"
}

need_systemd_run() {
  need systemd-run "the bench holds each run to a budget in a systemd scope, which needs systemd's user service manager (Linux only)"
}

need_timeout() {
  need timeout "install GNU coreutils' timeout (macOS: brew install coreutils, which names it gtimeout: put its gnubin first on PATH)"
}

# Whether ljos bench's options leave Qt in (it is, unless --runtime names
# runtimes without qt).
bench_has_qt() {
  local want=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --runtime) want="${2:-}"; shift ;;
      --runtime=*) want="${1#--runtime=}" ;;
    esac
    shift
  done
  case ",${want:-qt}," in
    *,qt,*) return 0 ;;
    *) return 1 ;;
  esac
}

# The package version a vendored crate's manifest declares: its first
# `version = "…"` line, which is the package's (its dependencies' come later).
# sed stops there itself; `| head -n 1` could end sed early under pipefail.
crate_version() {
  local v
  v="$(sed -n '/^version = "/{s/^version = "\(.*\)"$/\1/p;q;}' "$1")"
  [ -n "$v" ] || die "$1 declares no version"
  printf '%s\n' "$v"
}

# The --check of a task that opens a window: the same `ljos run`, with no
# window and no display, for LJOS_CHECK_SECONDS (2 by default), then ended by
# closing its standard input. It fails when the app does not build or load.
#   check_run <ljos> run [run options] <file> [args…]
check_run() {
  local ljos="$1"
  shift
  [ "${1:-}" = run ] || die "check_run: $* is not an ljos run"
  shift
  echo "${MISE_TASK_NAME:-this task} --check: ljos run --headless $*, ended after ${LJOS_CHECK_SECONDS:-2} s" >&2
  sleep "${LJOS_CHECK_SECONDS:-2}" | "$ljos" run --headless --quit-with-stdin "$@"
}

