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
    die "$(task_name) needs $1, which is not on PATH: $2"
}

# A directory with its links resolved.
physical() {
  (cd "$1" && pwd -P)
}

# Whether mise is running one of this checkout's tasks. A program started from
# a mise task elsewhere (an editor or agent under another project's `mise
# run`) hands all of mise's variables down, MISE_TASK_NAME too, to what it
# runs without mise (.mcp.json's `bash mise-tasks/mcp .`). So mise's variables
# count only when MISE_TASK_FILE is in this checkout's mise-tasks/, which a
# task this one calls directly (features calls features/run) shares.
own_task() {
  local tasks file
  [ -n "${MISE_TASK_FILE:-}" ] &&
    tasks="$(physical "$ljos_root/mise-tasks" 2>/dev/null)" &&
    file="$(physical "$(dirname "$MISE_TASK_FILE")" 2>/dev/null)" || return 1
  case "$file/" in
    "$tasks"/*) return 0 ;;
  esac
  return 1
}

# Where the person was: mise runs a task in the repository's root and says
# where it was started in MISE_ORIGINAL_CWD (own_task).
asked_in() {
  if [ -n "${MISE_ORIGINAL_CWD:-}" ] && own_task; then
    printf '%s\n' "$MISE_ORIGINAL_CWD"
  else
    printf '%s\n' "$PWD"
  fi
}

# The task's name for its messages: mise's (own_task), else the one its path
# under mise-tasks/ gives (check/wasm is check:wasm), else "this task".
task_name() {
  local rel
  if [ -n "${MISE_TASK_NAME:-}" ] && own_task; then
    printf '%s\n' "$MISE_TASK_NAME"
    return
  fi
  case "$0" in
    mise-tasks/* | */mise-tasks/*)
      rel="${0##*mise-tasks/}"
      rel="${rel%/_default}"
      printf '%s\n' "${rel//\//:}"
      ;;
    *) printf 'this task\n' ;;
  esac
}

# A path a person typed, absolute from where they typed it (asked_in). It
# need not exist yet.
from_asked() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s/%s\n' "$(asked_in)" "$1" ;;
  esac
}

# --- Where cargo builds ----------------------------------------------------------

# The main checkout of the repository whose top <dir> is, from what git
# keeps in the repository's common git directory, whatever its name: a
# directory git names in core.worktree there (a submodule's), else the one
# holding it when it is a .git. A bare repository (core.bare) has no main
# checkout, and git keeps no word of where the checkout of a git directory
# made elsewhere (git init --separate-git-dir) is; so those worktrees, like
# a checkout that is no git checkout or is only a directory inside one, are
# their own main checkouts.
main_checkout() {
  local dir top common tree
  dir="$(physical "$1")"
  top="$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null || true)"
  if [ -n "$top" ] && [ -d "$top" ] && [ "$(physical "$top")" = "$dir" ]; then
    common="$(git -C "$dir" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)"
    if [ -n "$common" ] && [ -d "$common" ] &&
      [ "$(git --git-dir="$common" rev-parse --is-bare-repository 2>/dev/null)" = false ]; then
      tree="$(git --git-dir="$common" config core.worktree 2>/dev/null || true)"
      if [ -n "$tree" ]; then
        tree="$(cd "$common" && cd "$tree" 2>/dev/null && pwd -P || true)"
      elif [ "$(basename "$common")" = .git ]; then
        tree="$(dirname "$common")"
      fi
      if [ -n "$tree" ] && [ -d "$tree" ]; then
        physical "$tree"
        return 0
      fi
    fi
  fi
  printf '%s\n' "$dir"
}

# toml_unescape <text>: a TOML basic string's escapes decoded (\" \\ \b \t
# \n \f \r \e, \xHH, \uHHHH and \UHHHHHHHH). Only a setting with a backslash
# in it comes here, so only that needs python3. (need_python3 is defined
# below, after this runs as env.sh is sourced.)
toml_unescape() {
  need python3 "install Python 3 (Arch: pacman -S python; Debian, Ubuntu: apt install python3; macOS: brew install python) to read the escapes in a cargo target-dir setting"
  python3 -I -c '
import re, sys
plain = {"b": "\b", "t": "\t", "n": "\n", "f": "\f", "r": "\r", "e": "\x1b", "\"": "\"", "\\": "\\"}
def one(m):
    e = m.group(1)
    return chr(int(e[1:], 16)) if e[0] in "xuU" else plain.get(e, "\\" + e)
sys.stdout.write(re.sub(r"\\(x[0-9A-Fa-f]{2}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8}|.)", one, sys.argv[1]))
' "$1"
}

# settings_target_dir <config.toml>: the build.target-dir a cargo settings
# file names, from the directory that holds its .cargo as cargo reads it, or
# nothing. A line-wise reading rather than a TOML parser, since macOS's
# python3 has no tomllib: a [build] table's target-dir, a dotted
# build.target-dir, or one in an inline `build = { … }`, with any part of the
# key quoted either way; the last one wins. A basic ("…") string's escapes
# are decoded as cargo decodes them, a literal ('…') one is taken as it is.
# Multi-line strings are not read: no path needs one.
settings_target_dir() {
  local file="$1" value
  [ -f "$file" ] || return 0
  value="$(awk '
    function bare(s) { gsub(/["'\'' \t]/, "", s); return s }
    # The string at the start of s: "b:" or "l:" and what is between its quotes.
    function string(s,   q, i, c) {
      q = substr(s, 1, 1)
      if (q == "'\''") { i = index(substr(s, 2), q); return i > 0 ? "l:" substr(s, 2, i - 1) : "" }
      if (q != "\"") return ""
      for (i = 2; i <= length(s); i++) {
        c = substr(s, i, 1)
        if (c == "\\") i++
        else if (c == "\"") return "b:" substr(s, 2, i - 2)
      }
      return ""
    }
    { sub(/\r$/, "") }
    /^[ \t]*#/ { next }
    /^[ \t]*\[/ { table = $0; sub(/\].*$/, "", table); gsub(/\[/, "", table); table = bare(table); next }
    {
      eq = index($0, "=")
      if (eq == 0) next
      key = bare(substr($0, 1, eq - 1)); value = substr($0, eq + 1); sub(/^[ \t]*/, "", value)
      if (table == "build" && key == "target-dir") s = string(value)
      else if (table == "" && key == "build.target-dir") s = string(value)
      else if (table == "" && key == "build" && match(value, /[{,][ \t]*["'\'']?target-dir["'\'']?[ \t]*=[ \t]*/)) s = string(substr(value, RSTART + RLENGTH))
      else next
      if (s != "") found = s
    }
    END { if (length(found) > 2) print found }
  ' "$file")"
  case "$value" in
    b:*\\*) value="$(toml_unescape "${value#b:}")" ;;
    *) value="${value#?:}" ;;
  esac
  case "$value" in
    "") ;;
    /*) printf '%s\n' "$value" ;;
    *) printf '%s/%s\n' "$(dirname "$(dirname "$file")")" "$value" ;;
  esac
}

# settings_file <dir>: the settings file in <dir>/.cargo, which cargo takes
# without the extension when both are there, or nothing.
settings_file() {
  if [ -f "$1/.cargo/config" ]; then
    printf '%s\n' "$1/.cargo/config"
  elif [ -f "$1/.cargo/config.toml" ]; then
    printf '%s\n' "$1/.cargo/config.toml"
  fi
}

# settings_files <dir>: the settings files cargo reads when it runs in <dir>,
# the one that binds most first: <dir>'s, every directory's above it, then
# cargo's home's.
settings_files() {
  local dir="$1" home="${CARGO_HOME:-$HOME/.cargo}"
  while :; do
    settings_file "$dir"
    if [ "$dir" = / ] || [ -z "$dir" ]; then break; fi
    dir="$(dirname "$dir")"
  done
  if [ -f "$home/config" ]; then
    printf '%s\n' "$home/config"
  elif [ -f "$home/config.toml" ]; then
    printf '%s\n' "$home/config.toml"
  fi
}

# target_dir [dir]: where cargo builds for the checkout at <dir> (this one by
# default), the one place that is decided, in cargo's own order:
# CARGO_TARGET_DIR, then CARGO_BUILD_TARGET_DIR (a relative one of either is
# from the checkout, where the tasks build), then the first target-dir in the
# settings cargo reads, so a target put on a drive with room stays there. A
# linked worktree's own settings come first, then those its main checkout
# builds with (its own, the directories' above it, cargo's home), then the
# worktree's surroundings. Else the main checkout's target/, so a linked
# worktree reuses the build that is there rather than growing one of its own
# wherever the worktree was made. A target that is a link (to a drive with
# room, say) is where it leads.
target_dir() {
  local dir="${1:-$ljos_root}" asked="${CARGO_TARGET_DIR:-${CARGO_BUILD_TARGET_DIR:-}}" target main
  case "$asked" in
    "")
      main="$(main_checkout "$dir")"
      local file
      while IFS= read -r file; do
        target="$(settings_target_dir "$file")"
        if [ -n "$target" ]; then break; fi
      done <<EOF_SETTINGS
$(settings_file "$dir")
$(settings_files "$main")
$(settings_files "$dir")
EOF_SETTINGS
      if [ -z "$target" ]; then target="$main/target"; fi
      ;;
    /*) target="$asked" ;;
    *) target="$dir/$asked" ;;
  esac
  if [ -d "$target" ]; then
    target="$(physical "$target")"
  fi
  printf '%s\n' "$target"
}

# cargo_config [dir]: the settings files to give cargo with --config, one
# per line, when <dir> is a linked worktree. Cargo looks for its settings
# from where it runs upwards, so a worktree made elsewhere would build
# without the ones its main checkout keeps outside git (a compile cache, a
# cap on jobs, incremental builds off, in the checkout's .cargo or a
# directory above it), and the two would keep rebuilding each other's crates
# in the target they share. So: every file cargo reads for the main checkout
# that it does not read in the worktree, the farthest first, as a later
# --config binds more. A worktree with settings of its own gets them again
# last, so its keys still win over its main checkout's (an array in them,
# such as rustflags, then comes twice). Nothing for a main checkout, whose
# settings cargo finds itself.
cargo_config() {
  local dir main here file given=""
  dir="$(physical "${1:-$ljos_root}")"
  main="$(main_checkout "$dir")"
  [ "$main" != "$dir" ] || return 0
  here="$(settings_files "$dir")"
  while IFS= read -r file; do
    if [ -z "$file" ] || printf '%s\n' "$here" | grep -qxF "$file"; then continue; fi
    given="$file${given:+
$given}"
  done <<EOF_MAIN
$(settings_files "$main")
EOF_MAIN
  [ -n "$given" ] || return 0
  printf '%s\n' "$given"
  file="$(settings_file "$dir")"
  if [ -n "$file" ]; then printf '%s\n' "$file"; fi
}

# config_args <files>: a --config for each line of <files>, in order, into
# the array config_args.
config_args() {
  local file
  config_args=()
  while IFS= read -r file; do
    if [ -n "$file" ]; then config_args+=(--config "$file"); fi
  done <<EOF_FILES
$1
EOF_FILES
}

# Every task builds where target_dir says, and so does every cargo its cargo
# starts (ljos_bdd's builds of ljos, say), through the variable. What was
# asked for before that is kept for a task that builds another checkout
# (mise-tasks/mcp).
# shellcheck disable=SC2034 # read by mise-tasks/mcp
ljos_asked_target_dir="${CARGO_TARGET_DIR-}"
CARGO_TARGET_DIR="$(target_dir)"
export CARGO_TARGET_DIR
ljos_cargo_config="$(cargo_config)"

# cargo, with the main checkout's settings in a worktree. --config outranks
# CARGO_TARGET_DIR, so a target-dir in those files would win over it: the
# target is given again after it, as a TOML string. A task runs `cargo`, never
# `exec cargo` or `command cargo`, which would go round this (tasks:smoke
# holds every task to that).
cargo() {
  local t config_args
  if [ -n "$ljos_cargo_config" ]; then
    config_args "$ljos_cargo_config"
    t="${CARGO_TARGET_DIR//\\/\\\\}"
    t="${t//\"/\\\"}"
    command cargo "${config_args[@]}" --config "build.target-dir=\"$t\"" "$@"
  else
    command cargo "$@"
  fi
}

# Empty when .mcp.json (<file>) starts its ljos server through mise-tasks/mcp,
# which builds where target_dir says; otherwise what is wrong, with the fix.
# A bare `cargo run` there builds into the worktree's own target/ every time
# an editor or agent starts in a new worktree.
mcp_json_problem() {
  need_python3
  python3 -I - "$1" <<'EOF_MCP'
import json, sys
path = sys.argv[1]
fix = 'start it with "command": "bash", "args": ["mise-tasks/mcp", "."] (or mise run mcp), which builds where the tasks do'
def problem(what):
    print("%s: %s" % (what, fix))
    sys.exit(0)
try:
    server = json.load(open(path)).get("mcpServers", {}).get("ljos")
except (OSError, ValueError, AttributeError) as e:
    problem("%s cannot be read (%s)" % (path, e))
if not isinstance(server, dict):
    problem("%s has no ljos server" % path)
args = server.get("args", [])
if not isinstance(args, list):
    problem("%s's ljos server has args that are not a list (%s)" % (path, json.dumps(args)))
run = [str(server.get("command", ""))] + [str(a) for a in args]
launcher = ("mise-tasks/mcp", "./mise-tasks/mcp")
# The task is a bash script: sh, which may be dash, would stop at its first
# bash-only line.
if run[0] == "sh" and len(run) > 1 and run[1] in launcher:
    problem("%s's ljos server runs mise-tasks/mcp with sh, which need not be bash" % path)
if not (run[0] in launcher
        or (run[0] == "bash" and len(run) > 1 and run[1] in launcher)
        or (run[0] == "mise" and run[1:3] == ["run", "mcp"])):
    problem("%s's ljos server runs `%s`, not mise-tasks/mcp, so in a linked worktree it builds into that worktree's own target/"
            % (path, " ".join(run)))
EOF_MCP
}

# shell_code <file>: a shell script with its comment lines left out and its
# continued lines joined, one command line per line.
shell_code() {
  awk '
    /^[ \t]*#/ { next }
    { line = line $0 }
    /\\$/ { sub(/\\$/, "", line); next }
    { print line; line = "" }
    END { if (line != "") print line }
  ' "$1"
}

# cargo_problems <task> [root]: what is wrong with how a task runs cargo,
# for tasks:smoke to list, or nothing. A task's cargo builds where
# target_dir says, with the main checkout's settings in a worktree, only
# through env.sh's cargo: a task that runs cargo sources env.sh, and never
# goes round its cargo with exec or command. A script of the checkout's
# scripts/ (under <root>, this checkout by default) that the task names and
# that runs cargo runs it for the task: env.sh's CARGO_TARGET_DIR is what
# puts that build in the shared target, so the task sources env.sh for it.
# cargo counts only where a command starts (after a separator, a keyword
# such as if, or assignments), so a message that names a cargo command runs
# none.
cargo_problems() {
  local code pre runs="" script
  code="$(shell_code "$1")"
  pre='(^|[;&|({`])[[:space:]]*((then|do|else|elif|if|while|until|!|time)[[:space:]]+)*([A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+)*'
  if printf '%s\n' "$code" | grep -Eq "${pre}((exec|command)[[:space:]]+)?cargo([[:space:]]|$)"; then
    runs=1
  else
    while IFS= read -r script; do
      case "$script" in *.py | *.qml) continue ;; esac
      [ -f "${2:-$ljos_root}/$script" ] || continue
      if shell_code "${2:-$ljos_root}/$script" | grep -Eq "${pre}((exec|command)[[:space:]]+)?cargo([[:space:]]|$)"; then
        runs=1
        break
      fi
    done <<EOF_SCRIPTS
$(printf '%s\n' "$code" | grep -oE 'scripts/[A-Za-z0-9_.-]+' | sort -u)
EOF_SCRIPTS
  fi
  [ -n "$runs" ] || return 0
  printf '%s\n' "$code" | grep -Eq '^[[:space:]]*(\.|source)[[:space:]].*_lib/env\.sh"?[[:space:]]*$' ||
    printf ' runs cargo without sourcing _lib/env.sh, so it builds into a worktree'"'"'s own target/;'
  printf '%s\n' "$code" | grep -Eq "${pre}(exec|command)[[:space:]]+cargo([[:space:]]|$)" &&
    printf ' exec cargo or command cargo goes round env.sh'"'"'s cargo (run cargo);'
  return 0
}

# checkout_dir <name> [dir]: where the runs of the checkout at <dir> (this
# one by default) keep what they write under <name> (a report, a sandbox's
# home): in the target directory, and for a linked worktree, which shares
# that with its main checkout, in worktrees/<its name>-<a sum of its path>/
# there, so two checkouts' runs never write over or clear each other's.
checkout_dir() {
  local dir target
  dir="$(physical "${2:-$ljos_root}")"
  target="$(target_dir "$dir")"
  if [ "$(main_checkout "$dir")" = "$dir" ]; then
    printf '%s/%s\n' "$target" "$1"
  else
    printf '%s/worktrees/%s-%s/%s\n' "$target" "$(basename "$dir")" "$(printf '%s' "$dir" | cksum | awk '{ print $1 }')" "$1"
  fi
}

# built_programs: from cargo's JSON messages on stdin, the path of every
# program the build made, one per line. The path is a JSON string: a quote,
# a backslash or a slash in it comes escaped. Any other escape is a control
# character, which no path of a build has; it is left, and the path is then
# no program.
built_programs() {
  sed -n -E 's/^\{"reason":"compiler-artifact".*"executable":"(([^"\\]|\\.)*)".*/\1/p' |
    sed -E 's/\\(["\\/])/\1/g'
}

# pin_build <cargo args…>: cargo with its JSON messages on stdout; any other
# line it prints there goes to stderr.
pin_build() {
  local out
  out="$(cargo "$@" --message-format=json-render-diagnostics)" || return 1
  printf '%s\n' "$out" | grep -v -e '^{"reason":' -e '^$' >&2 || true
  printf '%s\n' "$out" | grep '^{"reason":' || true
}

# pin_programs <dir> <cargo args…>: builds, gives the run its own link to
# every program the build made, in <dir>, and prints "<name> <path>" for
# each. A target directory may be shared (a linked worktree's is its main
# checkout's), and another checkout's build while a long run goes on would
# put that checkout's programs where the run's were. A hard link keeps the
# file it was made to, as cargo and the linker put new files in place of old
# ones (a <dir> on another drive than the target gets a copy). The links
# are this build's only if no other build came between it and them: so
# cargo builds again, and only when it finds every program fresh are they
# kept; else they are made again from that build.
pin_programs() {
  local into="$1" messages exe name pinned tries=0
  shift
  mkdir -p "$into"
  messages="$(pin_build "$@")" || return 1
  while :; do
    pinned=""
    while IFS= read -r exe; do
      [ -n "$exe" ] || continue
      name="$(basename "$exe")"
      ln -f "$exe" "$into/$name" 2>/dev/null || cp -p "$exe" "$into/$name" || return 1
      pinned="$pinned$name $into/$name
"
    done <<EOF_PROGRAMS
$(printf '%s\n' "$messages" | built_programs)
EOF_PROGRAMS
    messages="$(pin_build "$@")" || return 1
    printf '%s\n' "$messages" | grep -q '^{"reason":"compiler-artifact".*"executable":".*"fresh":false' || break
    tries=$((tries + 1))
    if [ "$tries" -ge 10 ]; then
      printf '%s\n' "${MISE_TASK_NAME:-this task}: other builds into $CARGO_TARGET_DIR kept coming between this build and its programs; build when they are done, or set CARGO_TARGET_DIR to a target of this checkout's own" >&2
      return 1
    fi
  done
  printf '%s' "$pinned"
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
    die "$(task_name) needs the qtdoc corpus, which is not checked out: git submodule update --init --depth 1 corpus/qtdoc (or mise run setup)"
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
  die "$(task_name) needs Qt 6's qmltestrunner, which was not found: install Qt 6's QML test tools (Arch: pacman -S qt6-declarative; Debian, Ubuntu: apt install qt6-declarative-dev-tools), or set QMLTESTRUNNER"
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
# With programs named (QML6's path, say), only those are asked; each is told
# apart by its name.
qt_found() {
  local tool v
  [ $# -gt 0 ] || set -- qmake6 qtpaths6 qtpaths qml6
  for tool in "$@"; do
    command -v "$tool" >/dev/null 2>&1 || continue
    case "${tool##*/}" in
      qmake*) v="$("$tool" -query QT_VERSION 2>/dev/null || true)" ;;
      qml*) v="$(QT_QPA_PLATFORM=offscreen "$tool" --version 2>/dev/null | sed -n 's/^Qml Runtime //p' | sed -n 1p || true)" ;;
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

# qt_other <pin> <inventory>: of the "<tool> <version>" lines qt_found gave,
# those that are not the pin's minor version, as one "<version> <tool>[,
# <tool>…]" line per version, in the order they came. The caller asks qt_pin
# and qt_found once, so what it reports is what it classified. The version is
# the last field: a tool's path may have spaces.
qt_other() {
  printf '%s\n' "$2" | awk -v want="${1%.*}" '
    NF >= 2 && $NF != want && index($NF, want ".") != 1 {
      v = $NF
      tool = substr($0, 1, length($0) - length(v) - 1)
      if (!(v in tools)) order[++n] = v
      tools[v] = (v in tools) ? tools[v] ", " tool : tool
    }
    END { for (i = 1; i <= n; i++) printf "%s %s\n", order[i], tools[order[i]] }'
}

# qt_mismatch [program]: empty when the Qt a task runs is CI's minor version
# (or no Qt 6 answers); otherwise one line per other version, naming the
# tools that report it, with the fix. With a program (the qml6 the task runs,
# QML6 or not), that program is the one asked; without, every Qt 6 tool on
# PATH. Fails, having said why, when ci.yml's pin cannot be read.
qt_mismatch() {
  local pin inventory version tools
  pin="$(qt_pin)" || return 1
  inventory="$(qt_found "$@")"
  qt_other "$pin" "$inventory" | while read -r version tools; do
    printf "Qt %s (%s) is not CI's Qt %s (.github/workflows/ci.yml pins %s), so what it gives here can differ from what CI compares with: %s\n" \
      "$version" "$tools" "${pin%.*}" "$pin" "$(qt_pin_fix "$pin")"
  done
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
  [ -z "$why" ] || die "$(task_name): $why"
}

# need_rust_target <triple>: stops a task that builds for a target the Rust in
# use lacks, before cargo's longer "can't find crate for `core`". A Rust that
# rustup does not manage is left to cargo, which says what it lacks itself.
need_rust_target() {
  local installed
  installed="$(rustup target list --installed 2>/dev/null)" || return 0
  case "
$installed
" in
    *"
$1
"*) ;;
    *) die "$(task_name): Rust $(rust_pin version) has no $1 target: run mise run setup, or rustup target add $1 --toolchain $(rust_pin version)" ;;
  esac
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

# The qml6 a bench measures: the last --qml6 among its options, else QML6,
# else qml6. The preflight, the report's note and the runs all use it.
bench_qml6() {
  local qml="${QML6:-qml6}"
  while [ $# -gt 0 ]; do
    case "$1" in
      --qml6) qml="${2:-}"; shift ;;
      --qml6=*) qml="${1#--qml6=}" ;;
    esac
    shift
  done
  printf '%s\n' "$qml"
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
  echo "$(task_name) --check: ljos run --headless $*, ended after ${LJOS_CHECK_SECONDS:-2} s" >&2
  sleep "${LJOS_CHECK_SECONDS:-2}" | "$ljos" run --headless --quit-with-stdin "$@"
}


# The program a cargo build made, from its --message-format=json messages on
# standard input: the last compiler artifact with an executable. Cargo knows
# where it put it (a target-dir from its config, a target triple's
# directory), which a path made from target_dir does not.
#
# Its path is a JSON string, decoded with awk alone (this runs before anything
# is built, so jq or python may not be there). A directory's name may hold a
# quote, a backslash or a control character (a tab, say), all of which come
# escaped. Cargo writes any other character as it is, so a \u escape above
# \u007f is not one it makes: the path is then left empty, and no program.
json_string() {
  awk '
    function hex(c) { return index("0123456789abcdef", tolower(c)) - 1 }
    {
      s = $0; out = ""
      while ((i = index(s, "\\")) > 0) {
        out = out substr(s, 1, i - 1); c = substr(s, i + 1, 1); s = substr(s, i + 2)
        if (c == "\"" || c == "\\" || c == "/") out = out c
        else if (c == "b") out = out "\b"
        else if (c == "f") out = out "\f"
        else if (c == "n") out = out "\n"
        else if (c == "r") out = out "\r"
        else if (c == "t") out = out "\t"
        else if (c == "u" && substr(s, 1, 2) == "00" && hex(substr(s, 3, 1)) >= 0 && hex(substr(s, 3, 1)) < 8 && hex(substr(s, 4, 1)) >= 0) {
          n = hex(substr(s, 3, 1)) * 16 + hex(substr(s, 4, 1))
          if (n == 0) exit
          out = out sprintf("%c", n); s = substr(s, 5)
        } else exit
      }
      printf "%s", out s
    }'
}
cargo_executable() {
  sed -n -E 's/^\{"reason":"compiler-artifact".*"executable":"(([^"\\]|\\.)*)".*/\1/p' |
    tail -n 1 | json_string
}

# Builds with cargo and prints the program the build made:
#   built_program -p ljos_cli --bin ljos [--profile release]
# The build's diagnostics go to stderr as cargo renders them.
built_program() {
  local messages program
  messages="$(cargo build --quiet --message-format=json-render-diagnostics "$@")" || return 1
  program="$(printf '%s\n' "$messages" | cargo_executable)"
  [ -n "$program" ] && [ -x "$program" ] || die "cargo build $* named no program it made"
  printf '%s\n' "$program"
}

# The desktop:cua sandbox's home (scripts/cua-sandbox.sh), one per checkout:
# a linked worktree shares its main checkout's target, and a sandbox started
# from one would end the app another drives.
LJOS_CUA_HOME="${LJOS_CUA_HOME:-$(checkout_dir cua)}"
export LJOS_CUA_HOME
