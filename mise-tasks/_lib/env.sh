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

# own_target_dir <config.toml>: the build.target-dir a cargo settings
# file names itself (not through an include), from the directory that holds
# its .cargo as cargo reads it, or nothing. A line-wise reading rather than a TOML parser, since macOS's
# python3 has no tomllib: a [build] table's target-dir, a dotted
# build.target-dir, or one in an inline `build = { … }`, with any part of the
# key quoted either way; the last one wins. A basic ("…") string's escapes
# are decoded as cargo decodes them, a literal ('…') one is taken as it is.
# Multi-line strings are not read: no path needs one.
own_target_dir() {
  local file="$1" value
  [ -f "$file" ] || return 0
  value="$(awk '
    # A key as cargo reads it: blanks outside quotes left out, a quoted
    # part kept as it is (["build "] is not [build]), and a dot inside
    # quotes part of the name, not a separator.
    function bare(s,   out, i, c, q) {
      out = ""; q = ""
      for (i = 1; i <= length(s); i++) {
        c = substr(s, i, 1)
        if (q != "") {
          if (c == q) q = ""
          else if (q == "\"" && c == "\\") { out = out c substr(s, i + 1, 1); i++ }
          else out = out (c == "." ? "\034" : c)
        } else if (c == "\"" || c == "'\''") q = c
        else if (c != " " && c != "\t") out = out c
      }
      return out
    }
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
  # A target-dir that cannot be decoded stops here: taken as no setting, the
  # build would go to target/, the drive the setting moved it off.
  value="$(toml_text "$value")" || {
    printf '%s\n' "$(task_name): $file: its build.target-dir's escapes cannot be decoded (above), so where cargo builds cannot be told" >&2
    return 1
  }
  case "$value" in
    "") ;;
    /*) printf '%s\n' "$value" ;;
    *) printf '%s/%s\n' "$(dirname "$(dirname "$file")")" "$value" ;;
  esac
}

# settings_target_dir <config.toml>: the build.target-dir a cargo settings
# file names with what it includes, or nothing: the last one in the order
# settings_tree gives, so the file's own wins over its includes', and a
# later include's over an earlier one's. Each is from the directory that
# holds the .cargo of the file that names it.
settings_target_dir() {
  local tree file value found=""
  [ -f "$1" ] || return 0
  tree="$(settings_tree "$1")" || return 1
  while IFS= read -r file; do
    [ -n "$file" ] || continue
    value="$(own_target_dir "$file")" || return 1
    [ -z "$value" ] || found="$value"
  done <<EOF_TREE
$tree
EOF_TREE
  [ -z "$found" ] || printf '%s\n' "$found"
}

# settings_tree <config.toml> [files met already]: the file and those it
# includes, one per line in the order cargo merges them (cargo_settings'
# expand says how cargo 1.98.1 includes), or a line saying what stops cargo.
settings_tree() {
  local file="$1" seen="${2:-}" includes line item sub nl='
'
  # Files are met by where they are, so ../i/x.toml from i/x.toml is x.toml
  # again, not a new file each time round.
  seen="$seen$(physical "$(dirname "$file")")/${file##*/}$nl"
  includes="$(settings_includes "$file")" || return 1
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    item="$(toml_text "${line#*	}")" || {
      printf '%s\n' "$(task_name): $file: an include's escapes cannot be decoded (above)" >&2
      return 1
    }
    case "$item" in
      *.toml) ;;
      *) printf '%s\n' "$(task_name): $file: its include $item does not end with .toml, which cargo asks of an include" >&2; return 1 ;;
    esac
    case "$item" in /*) ;; *) item="$(dirname "$file")/$item" ;; esac
    case "$nl$seen" in
      *"$nl$(physical "$(dirname "$item")" 2>/dev/null)/${item##*/}$nl"*) printf '%s\n' "$(task_name): $file: its include $item comes round again (an include cycle), which cargo refuses" >&2; return 1 ;;
    esac
    if [ ! -f "$item" ]; then
      [ "${line%%	*}" = o ] && continue
      printf '%s\n' "$(task_name): $file: its include $item is not there; cargo stops on it" >&2
      return 1
    fi
    sub="$(settings_tree "$item" "$seen")" || return 1
    printf '%s\n' "$sub"
    while IFS= read -r line; do
      seen="$seen$(physical "$(dirname "$line")")/${line##*/}$nl"
    done <<EOF_SUB
$sub
EOF_SUB
  done <<EOF_INCLUDES
$includes
EOF_INCLUDES
  printf '%s\n' "$file"
}

# toml_text <b:text or l:text>: a string as own_target_dir's reading gives
# it, a basic one's escapes decoded as cargo decodes them.
toml_text() {
  case "$1" in
    b:*\\*) toml_unescape "${1#b:}" ;;
    *) printf '%s' "${1#?:}" ;;
  esac
}

# settings_includes <config.toml>: what a cargo settings file's top-level
# include lists, one per line in its order: "r" (or "o" for an optional
# one), a tab, then the path as own_target_dir's reading gives a string. An
# include that is not an array of strings and tables {path, optional} stops
# the task, as it stops cargo. Line-wise like own_target_dir; the array may
# span lines.
settings_includes() {
  local out
  out="$(awk '
    # str: the string that starts at s[i], "b:" or "l:" then its text; at
    # holds where its closing quote is.
    function str(s, i,   q, j, c) {
      q = substr(s, i, 1)
      for (j = i + 1; j <= length(s); j++) {
        c = substr(s, j, 1)
        if (q == "\"" && c == "\\") { j++; continue }
        if (c == q) break
      }
      at = j
      return (q == "\"" ? "b:" : "l:") substr(s, i + 1, j - i - 1)
    }
    # closed: whether text holds a whole array, quotes and comments skipped.
    function closed(text,   depth, q, i, c) {
      depth = 0; q = ""
      for (i = 1; i <= length(text); i++) {
        c = substr(text, i, 1)
        if (q != "") { if (q == "\"" && c == "\\") i++; else if (c == q) q = "" }
        else if (c == "\"" || c == "'\''") q = c
        else if (c == "#") { while (i <= length(text) && substr(text, i, 1) != "\n") i++ }
        else if (c == "[" || c == "{") depth++
        else if ((c == "]" || c == "}") && --depth == 0) return i
      }
      return 0
    }
    function bad() { print "!"; done = 1; exit }
    # One element of the array: a string, or { path = "…", optional = … }.
    function inline(t,   i, n, c, key, path, opt, w) {
      n = length(t); path = ""; opt = "r"; key = ""
      for (i = 2; i < n; i++) {
        c = substr(t, i, 1)
        if (c ~ /[ \t\n,]/) continue
        if (key == "") {
          if (c == "\"" || c == "'\''") { key = substr(str(t, i), 3); i = at; continue }
          w = substr(t, i); match(w, /^[A-Za-z0-9_-]+/); if (RLENGTH <= 0) bad()
          key = substr(w, 1, RLENGTH); i += RLENGTH - 1; continue
        }
        if (c == "=") continue
        if (key == "path") { if (c != "\"" && c != "'\''") bad(); path = str(t, i); i = at }
        else if (key == "optional") {
          w = substr(t, i)
          if (w ~ /^true/) { opt = "o"; i += 3 } else if (w ~ /^false/) i += 4; else bad()
        } else bad()
        key = ""
      }
      if (path == "") bad()
      print opt "\t" path
    }
    { sub(/\r$/, "") }
    !open && /^[ \t]*#/ { next }
    !open && /^[ \t]*\[/ { table = 1; next }
    !open && !table && match($0, /^[ \t]*("include"|'\''include'\''|include)[ \t]*=[ \t]*/) {
      text = substr($0, RSTART + RLENGTH); open = 1
      if (substr(text, 1, 1) != "[") bad()
    }
    open && !started { started = 1; if (!closed(text)) next }
    open && started && !closed(text) { text = text "\n" $0; if (!closed(text)) next }
    open {
      end = closed(text); value = substr(text, 2, end - 2); n = length(value)
      for (i = 1; i <= n; i++) {
        c = substr(value, i, 1)
        if (c ~ /[ \t\n,]/) continue
        if (c == "#") { while (i <= n && substr(value, i, 1) != "\n") i++; continue }
        if (c == "\"" || c == "'\''") { print "r\t" str(value, i); i = at; continue }
        if (c != "{") bad()
        j = closed(substr(value, i)); if (!j) bad()
        inline(substr(value, i, j)); i += j - 1
      }
      done = 1; exit
    }
  ' "$1")"
  case "$out" in
    *'!'*) printf '%s\n' "$(task_name): $1: its include is not an array of paths and tables {path, optional}, as cargo takes it" >&2; return 1 ;;
  esac
  [ -z "$out" ] || printf '%s\n' "$out"
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
        target="$(settings_target_dir "$file")" || return 1
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

# cargo_config [dir]: the main checkout's cargo settings that <dir> builds
# with when it is a linked worktree, one per line, for config_args. Cargo
# looks for its settings from where it runs upwards, so a worktree made
# elsewhere would build without the ones its main checkout keeps outside git
# (a compile cache, a cap on jobs, incremental builds off, in the checkout's
# .cargo or a directory above it), and the two would keep rebuilding each
# other's crates in the target they share. So every file cargo reads for the
# main checkout that it does not read in the worktree is given, merged as
# cargo merges them (the nearest file's value wins, arrays are joined, the
# farthest first), key by key rather than as files:
# - a key the worktree's own settings set is theirs, as if cargo had found
#   both files there (given as a file again, the worktree's own arrays, such
#   as rustflags, would come twice). An array both set is joined, as cargo
#   joins it, the main checkout's after the worktree's: a --config ranks
#   above the files;
# - a --config outranks the environment, so config_args leaves a key out
#   while an exported variable sets it (CARGO_BUILD_JOBS, say, or
#   CARGO_TARGET_DIR, which env.sh sets): the environment wins, as in plain
#   cargo. An array is kept, as cargo adds the environment's to the files'
#   (RUSTFLAGS, which replaces every rustflags, leaves rustflags out);
# - a path the file names relative to it is made whole, as the settings
#   reach cargo from a file of their own somewhere else.
# A line is the variables that set its key instead, a tab, then the setting
# as a TOML line. Reading TOML takes Python 3.11's tomllib (or tomli);
# without it (macOS's python3 is 3.9), or when a file cannot be read, the
# files are given whole ("@file", a tab, the file), the farthest first, then
# the worktree's own again so that they still win, and the task says why
# and what that costs. Nothing for a main checkout, whose settings cargo
# finds itself.
cargo_config() {
  local dir main here mains file out why n=0 status=0 given=() own=() i
  dir="$(physical "${1:-$ljos_root}")"
  main="$(main_checkout "$dir")"
  [ "$main" != "$dir" ] || return 0
  here="$(settings_files "$dir")"
  mains="$(settings_files "$main")"
  while IFS= read -r file; do
    if [ -z "$file" ] || grep -qxF "$file" <<<"$here"; then continue; fi
    given=("$file" ${given[@]+"${given[@]}"})
  done <<EOF_MAIN
$mains
EOF_MAIN
  [ ${#given[@]} -gt 0 ] || return 0
  while IFS= read -r file; do
    if [ -z "$file" ] || grep -qxF "$file" <<<"$mains"; then continue; fi
    own+=("$file")
    n=$((n + 1))
  done <<EOF_HERE
$here
EOF_HERE
  if command -v python3 >/dev/null 2>&1; then
    out="$(cargo_settings "$n" ${own[@]+"${own[@]}"} "${given[@]}")" || status=$?
  else
    status=127
  fi
  if [ "$status" -eq 0 ]; then
    [ -z "$out" ] || printf '%s\n' "$out"
    return 0
  fi
  case "$status" in
    3) why="reading them key by key takes Python 3.11's tomllib (or tomli), which $(python3 -V 2>&1) has not" ;;
    4) why="$out" ;;
    5) printf '%s: %s\n' "$(task_name)" "$out" >&2; return 1 ;;
    127) why="reading them key by key takes python3, which is not on PATH" ;;
    *) why="python3 stopped with status $status reading them (above)" ;;
  esac
  printf '%s: %s; so the main checkout'\''s cargo settings (%s) are given as whole files, and this worktree'\''s own after them: an exported CARGO_* variable may be outranked by them, and an array both set comes more than once\n' \
    "$(task_name)" "$why" "$(printf '%s ' "${given[@]}" | sed 's/ $//')" >&2
  for file in "${given[@]}"; do
    printf '@file\t%s\n' "$file"
  done
  i=$n
  while [ "$i" -gt 0 ]; do
    i=$((i - 1))
    printf '@file\t%s\n' "${own[$i]}"
  done
}

# cargo_settings <n> <the worktree's own n files> <the main checkout's files,
# farthest first>: cargo_config's lines. Status 3 without tomllib; 4, with
# what failed as its output, when a file cannot be read; 5, likewise, on an
# include cargo stops on. Nothing is printed
# before every file is read, so a failure leaves no half.
cargo_settings() {
  python3 -I - "$@" <<'EOF_SETTINGS'
import json, os, re, sys
try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib
    except ImportError:
        sys.exit(3)

n = int(sys.argv[1])
own, given = sys.argv[2:2 + n], sys.argv[2 + n:]

def load(path):
    try:
        with open(path, "rb") as f:
            return tomllib.load(f)
    except OSError as e:
        print("%s cannot be read: %s" % (path, e.strerror or e))
    except ValueError as e:
        print("%s is not TOML cargo can read: %s" % (path, e))
    sys.exit(4)

# An include cargo stops on stops the task too, in one line: given whole,
# the files would only have cargo say it again after the task's note.
def fail(what):
    print(what)
    sys.exit(5)

# expand <file>: the file and what it includes, as cargo 1.98.1 reads them,
# in the order they merge (a later one's value wins): each include in the
# order listed, each with what it includes before it, then the file's own
# keys. An include is an array of paths ending in .toml, each a string or a
# table {path, optional}, from the directory of the file that names it;
# a missing one stops cargo unless it is optional. Cargo takes a file met a
# second time in one settings file's includes as a cycle, even one met
# through two different files, so seen spans the whole tree.
def expand(path, seen=None):
    path = os.path.abspath(path)
    seen = set() if seen is None else seen
    seen.add(os.path.realpath(path))
    table = load(path)
    includes = table.pop("include", [])
    if not isinstance(includes, list):
        fail("%s: its include is not an array; cargo takes a list of strings or of tables {path, optional}" % path)
    out = []
    for item in includes:
        optional = False
        if isinstance(item, dict):
            optional = item.get("optional", False)
            item = item.get("path")
            if not isinstance(optional, bool):
                fail("%s: an include's optional is not true or false" % path)
        if not isinstance(item, str):
            fail("%s: an include is neither a path nor a table with a path, as cargo takes it" % path)
        if not item.endswith(".toml"):
            fail("%s: its include %s does not end with .toml, which cargo asks of an include" % (path, item))
        target = os.path.abspath(os.path.join(os.path.dirname(path), item))
        if os.path.realpath(target) in seen:
            fail("%s: its include %s comes round again (an include cycle), which cargo refuses" % (path, item))
        if not os.path.exists(target):
            if optional:
                continue
            fail("%s: its include %s is not there (%s); cargo stops on it" % (path, item, target))
        out += expand(target, seen)
    return out + [(path, table)]

own = [t for f in own for _, t in expand(f)]
given = [g for f in given for g in expand(f)]

# A setting's leaves, each with the table it is in: an [env] entry's value
# and `relative` come apart, so the worktree can set one and not the other.
def leaves(table, path=()):
    for k, v in table.items():
        p = path + (k,)
        if isinstance(v, dict):
            yield from leaves(v, p)
        else:
            yield p, v, table

# The variables that set a key in place of the files: cargo's own name for
# it, for a value that is not an array (an array takes the variable's words
# as well), and the older names cargo still reads.
OTHER = {
    ("build", "rustc"): ["RUSTC"],
    ("build", "rustc-wrapper"): ["RUSTC_WRAPPER"],
    ("build", "rustc-workspace-wrapper"): ["RUSTC_WORKSPACE_WRAPPER"],
    ("build", "rustdoc"): ["RUSTDOC"],
    ("build", "target-dir"): ["CARGO_TARGET_DIR"],
    ("build", "incremental"): ["CARGO_INCREMENTAL"],
    ("build", "rustflags"): ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"],
    ("build", "rustdocflags"): ["RUSTDOCFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS"],
    ("http", "proxy"): ["HTTPS_PROXY", "https_proxy", "http_proxy"],
}
NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
def variables(p, v):
    names = list(OTHER.get(p, []))
    if not isinstance(v, list):
        names.insert(0, "CARGO_" + "_".join(k.upper().replace("-", "_").replace(".", "_") for k in p))
    return [name for name in names if NAME.match(name)]

# What cargo takes as a path relative to the directory that holds .cargo: a
# directory or file always (a build-dir that starts with a {template} is
# cargo's to place), a program only when it has a slash (else PATH finds
# it), and a command's program, its first word, the same.
def kind(p):
    if p in (("build", "target-dir"), ("build", "build-dir"), ("build", "dep-info-basedir"),
             ("install", "root"), ("http", "cainfo")):
        return "path"
    if len(p) == 3 and p[0] == "source" and p[2] in ("directory", "local-registry"):
        return "path"
    if len(p) == 4 and p[0] == "patch" and p[3] == "path":
        return "path"
    if p in (("build", "rustc"), ("build", "rustc-wrapper"), ("build", "rustc-workspace-wrapper"),
             ("build", "rustdoc")) or (p[0] in ("target", "host") and p[-1] == "linker"):
        return "program"
    if (p[0] in ("target", "host") and p[-1] == "runner") or p == ("doc", "browser") or \
            (p[0] in ("registry", "registries") and p[-1] == "credential-provider"):
        return "command"
    if p == ("registry", "global-credential-providers"):
        return "commands"
    if p == ("paths",):
        return "paths"
    return None

def whole(base, s):
    return s if os.path.isabs(s) else os.path.join(base, s)
def program(base, s):
    return whole(base, s) if "/" in s else s
def command(base, v):
    if isinstance(v, str):
        words = v.split()
        return " ".join([program(base, words[0])] + words[1:]) if words else v
    if isinstance(v, list) and v and isinstance(v[0], str):
        return [program(base, v[0])] + v[1:]
    return v
def placed(p, v, table, base):
    k = kind(p)
    if k == "path" and isinstance(v, str) and not v.startswith("{"):
        return whole(base, v)
    if k == "program" and isinstance(v, str):
        return program(base, v)
    if k == "command":
        return command(base, v)
    if k == "commands" and isinstance(v, list):
        return [command(base, x) for x in v]
    if k == "paths" and isinstance(v, list):
        return [whole(base, x) if isinstance(x, str) else x for x in v]
    if len(p) == 3 and p[0] == "env" and p[2] == "value" and table.get("relative") is True and isinstance(v, str):
        return whole(base, v)
    return v

BARE = re.compile(r"[A-Za-z0-9_-]+\Z")
def key(p):
    return ".".join(k if BARE.match(k) else string(k) for k in p)
def string(s):
    # A TOML basic string: JSON's escapes are TOML's, and DEL is escaped too.
    return json.dumps(s, ensure_ascii=False).replace("\x7f", "\\u007F")
def value(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return repr(v)
    if isinstance(v, list):
        return "[" + ", ".join(value(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{" + ", ".join("%s = %s" % (key((k,)), value(x)) for k, x in v.items()) + "}"
    return string(str(v))

def under(p, q):
    return p[:len(q)] == q or q[:len(p)] == p

# The worktree's own keys are its own, but an array both set is joined, as
# cargo joins the arrays of every file it reads.
mine = {p: v for d in own for p, v, _ in leaves(d)}
def theirs(p, v):
    for q, w in mine.items():
        if q == p:
            return isinstance(v, list) and isinstance(w, list)
        if under(p, q):
            return False
    return True

# One file's worth: a nearer file's value replaces a farther one's (and a
# table or value it puts in the place of one), arrays are joined.
merged = {}
for f, d in given:
    base = os.path.dirname(os.path.dirname(os.path.abspath(f)))
    for p, v, table in leaves(d):
        if not theirs(p, v):
            continue
        v = placed(p, v, table, base)
        for q in [q for q in merged if q != p and under(p, q)]:
            del merged[q]
        if isinstance(v, list) and isinstance(merged.get(p), list):
            v = merged[p] + v
        merged[p] = v
for p, v in merged.items():
    print("%s\t%s = %s" % (" ".join(variables(p, v)), key(p), value(v)))
EOF_SETTINGS
}

# toml_string <text>: <text> as a TOML basic string.
toml_string() {
  local t="$1"
  t="${t//\\/\\\\}"
  t="${t//\"/\\\"}"
  t="${t//$'\t'/\\t}"
  t="${t//$'\n'/\\n}"
  t="${t//$'\r'/\\r}"
  printf '"%s"' "$t"
}

# config_args <cargo_config output> <dir>: the --config arguments for its
# settings, into the array config_args. The settings no exported variable
# sets instead go into <dir>/settings.toml, a new file in a directory only
# the user can read, never into the arguments, where any user of the machine
# can read them (an [env] token, say). Whole files are given as they are,
# with the target CARGO_TARGET_DIR names and the build directory
# CARGO_BUILD_BUILD_DIR names after them, as they would outrank those.
config_args() {
  local line names name value overridden exported nl=$'\n' tab=$'\t' settings="" whole=""
  config_args=()
  # Only an exported variable reaches cargo (one given to this call as
  # `VAR=1 cargo` is exported to it).
  exported="$nl$(compgen -e)$nl"
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    names="${line%%"$tab"*}"
    value="${line#*"$tab"}"
    if [ "$names" = "@file" ]; then
      config_args+=(--config "$value")
      whole=1
      continue
    fi
    overridden=
    for name in $names; do
      case "$exported" in *"$nl$name$nl"*) overridden=1 && break ;; esac
    done
    [ -z "$overridden" ] || continue
    settings="$2/settings.toml"
    printf '%s\n' "$value" >>"$settings"
  done <<EOF_SETTINGS
$1
EOF_SETTINGS
  if [ -n "$settings" ]; then config_args+=(--config "$settings"); fi
  [ -n "$whole" ] || return 0
  case "$exported" in
    *"${nl}CARGO_TARGET_DIR$nl"*) config_args+=(--config "build.target-dir=$(toml_string "$CARGO_TARGET_DIR")") ;;
  esac
  case "$exported" in
    *"${nl}CARGO_BUILD_BUILD_DIR$nl"*) config_args+=(--config "build.build-dir=$(toml_string "$CARGO_BUILD_BUILD_DIR")") ;;
  esac
}

# Every task builds where target_dir says, and so does every cargo its cargo
# starts (ljos_bdd's builds of ljos, say), through the variable. What was
# asked for before that is kept for a task that builds another checkout
# (mise-tasks/mcp).
# shellcheck disable=SC2034 # read by mise-tasks/mcp
ljos_asked_target_dir="${CARGO_TARGET_DIR-}"
# Each failure says what stops it, in one line of its own.
CARGO_TARGET_DIR="$(target_dir)" || exit 1
export CARGO_TARGET_DIR
ljos_cargo_config="$(cargo_config)" || exit 1

# cargo, with the main checkout's settings in a worktree, from a settings
# file of its own that is gone once cargo is. A task runs `cargo` (or
# `exec_cargo`), never `exec cargo` or `command cargo`, which would go round
# this (tasks:smoke holds every task to that).
cargo() {
  local config_args settings status=0
  if [ -z "$ljos_cargo_config" ]; then
    command cargo "$@"
    return
  fi
  settings="$(mktemp -d "${TMPDIR:-/tmp}/ljos-cargo.XXXXXX")" || return
  config_args "$ljos_cargo_config" "$settings"
  command cargo ${config_args[@]+"${config_args[@]}"} "$@" || status=$?
  rm -f "$settings/settings.toml"
  rmdir "$settings"
  return "$status"
}

# exec_cargo <args…>: cargo in place of this shell, as `exec cargo` would run
# it (a signal sent to the task reaches cargo), with the settings `cargo`
# gives. No shell is left to remove the settings file after cargo, so it is
# opened on descriptor 9 and removed first, and cargo reads it as
# /dev/fd/9. Where opening /dev/fd/9 is not opening the file again (macOS
# has no /proc, and gives a copy of the descriptor that shares its offset
# with whatever read it before), the file stays in its directory, which only
# its user can read, under TMPDIR.
exec_cargo() {
  local config_args settings i
  if [ -z "$ljos_cargo_config" ]; then
    exec cargo "$@"
  fi
  settings="$(mktemp -d "${TMPDIR:-/tmp}/ljos-cargo.XXXXXX")" || exit
  config_args "$ljos_cargo_config" "$settings"
  if [ -f "$settings/settings.toml" ] && [ -d /proc/self/fd ]; then
    exec 9<"$settings/settings.toml" || exit
    for i in "${!config_args[@]}"; do
      [ "${config_args[$i]}" != "$settings/settings.toml" ] || config_args[i]=/dev/fd/9
    done
    rm -f "$settings/settings.toml"
  fi
  rmdir "$settings" 2>/dev/null || true
  # exec runs the program, never this file's cargo function.
  exec cargo ${config_args[@]+"${config_args[@]}"} "$@"
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
run = [server.get("command", "")] + args
if not all(isinstance(a, str) for a in run):
    problem("%s's ljos server has a command or args that are not a string (%s)" % (path, json.dumps(run)))
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

# command_words: shell_code's output with each quoted string made one word:
# a blank or separator inside quotes becomes _, and a string that runs on
# over lines is joined, so words in a message or a flag's value never start
# a command. A command substitution inside double quotes is code again. An
# inline comment is left out, and so is a here-document's text, up to its
# terminator: it is what a command reads, not commands (a `<<` inside
# `$(( ))` is a shift, and `<<<` a here-string, whose word is all it reads).
command_words() {
  awk '
    function neutral(c) { return c ~ /[ \t;&|()<>`{}]/ ? "_" : c }
    BEGIN { n = 0; st[0] = "c"; buf = ""; nh = 0; hi = 1 }
    hi <= nh {
      t = $0
      if (hs[hi]) sub(/^\t+/, "", t)
      if (t == hd[hi]) hi++
      if (hi > nh) { nh = 0; hi = 1 }
      next
    }
    {
      s = $0; len = length(s); out = ""; i = 1
      while (i <= len) {
        ch = substr(s, i, 1); two = substr(s, i, 2); top = st[n]
        if (top == "s") {
          if (ch == "\047") n--
          out = out neutral(ch); i++; continue
        }
        if (top == "a" || top == "d") {
          if (ch == "\\") { out = out "__"; i += 2; continue }
          if (top == "a" && ch == "\047") { n--; out = out ch; i++; continue }
          if (top == "d" && ch == "\"") { n--; out = out ch; i++; continue }
          if (top == "d" && two == "$(") { st[++n] = "c"; dp[n] = 1; out = out two; i += 2; continue }
          if (top == "d" && ch == "`") { st[++n] = "b"; out = out ch; i++; continue }
          out = out neutral(ch); i++; continue
        }
        if (ch == "\\") { out = out "__"; i += 2; continue }
        if (ch == "#" && (i == 1 || substr(s, i - 1, 1) ~ /[ \t;&|(]/)) break
        if (substr(s, i, 3) == "<<<") { out = out "<<<"; i += 3; continue }
        if (two == "<<" && !(top == "c" && n > 0 && dp[n] > 1)) {
          j = i + 2; dash = 0; w = ""
          if (substr(s, j, 1) == "-") { dash = 1; j++ }
          while (substr(s, j, 1) ~ /[ \t]/) j++
          q = substr(s, j, 1)
          if (q == "\047" || q == "\"") {
            k = index(substr(s, j + 1), q)
            if (k > 0) { w = substr(s, j + 1, k - 1); j += k + 1 }
          } else {
            if (q == "\\") j++
            if (substr(s, j, 1) ~ /[A-Za-z_]/) {
              while (substr(s, j, 1) ~ /[A-Za-z0-9_]/) { w = w substr(s, j, 1); j++ }
            }
          }
          if (w != "") { hd[++nh] = w; hs[nh] = dash; out = out "<<_"; i = j; continue }
        }
        if (two == "$\047") { st[++n] = "a"; out = out two; i += 2; continue }
        if (ch == "\047") st[++n] = "s"
        else if (ch == "\"") st[++n] = "d"
        else if (two == "$(") { st[++n] = "c"; dp[n] = 1; out = out two; i += 2; continue }
        else if (ch == "`") { if (top == "b") n--; else st[++n] = "b" }
        else if (ch == "(" && top == "c" && n > 0) dp[n]++
        else if (ch == ")" && top == "c" && n > 0 && --dp[n] == 0) n--
        out = out ch; i++
      }
      if (st[n] == "s" || st[n] == "a" || st[n] == "d") buf = buf out "_"
      else { print buf out; buf = "" }
    }
    END { if (buf != "") print buf }
  '
}

# cargo_problems <task> [root]: what is wrong with how a task runs cargo,
# for tasks:smoke to list, or nothing. A task's cargo builds where
# target_dir says, with the main checkout's settings in a worktree, only
# through env.sh's cargo: a task that runs cargo sources env.sh, and never
# goes round its cargo. exec, command and a program that runs another
# (timeout, nice, env, capped, mise exec, …) run the cargo program, not
# env.sh's cargo, so each of those counts as both. A script of the
# checkout's scripts/ (under <root>, this checkout by default) that the task
# names and that runs cargo is held to the same, as the script's own: a
# shell function does not reach the program a task runs, so the script's
# cargo is env.sh's only when the script sources env.sh itself (a task
# that sourced it would hand the script the target directory, but not the
# main checkout's settings). cargo counts only as a command's program:
# where a command starts (after a separator, a keyword such as if, or
# assignments), or as the program such a wrapper runs. A wrapper takes
# only its own options and their values (timeout its duration, taskset its
# mask or CPU list, chrt its priority), so the next word is the program:
# `exec "$prog" --note "cargo build"` runs $prog, and command -v cargo runs
# nothing. command_words makes quoted text one word, so a message that names
# a cargo command runs none. The text goes to grep as a here-string, not
# down a pipe: under pipefail, grep -q stopping early fails a long task's
# printf on the closed pipe, and the match with it.
cargo_problems() {
  local code pre via script s o v
  code="$(shell_code "$1")"
  pre='(^|[;&|({`])[[:space:]]*((then|do|else|elif|if|while|until|!|time)[[:space:]]+)*([A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+)*'
  s='[[:space:]]+'
  o="-[^[:space:];&|]*$s"
  v="[^[:space:];&|]+$s"
  via="((exec$s(-a$s$v|-[cl]+$s)*"
  via+="|command$s(-p$s)*"
  via+="|env$s((-u|-C|-S|--unset|--chdir|--split-string)$s$v|$o|[A-Za-z_][A-Za-z0-9_]*=[^[:space:];&|]*$s)*"
  via+="|nice$s((-n|--adjustment)$s$v|$o)*"
  via+="|(nohup|setsid)$s($o)*"
  via+="|timeout$s((-s|-k|--signal|--kill-after)$s$v|$o)*$v"
  via+="|stdbuf$s((-i|-o|-e)$s$v|$o)*"
  via+="|ionice$s((-c|-n|-p|-P|-u)$s$v|$o)*"
  via+="|(taskset|chrt)$s($o)*$v"
  via+="|xargs$s((-n|-I|-P|-L|-s|-d|-E|-a|--max-args|--replace|--max-procs|--max-lines|--max-chars|--delimiter|--eof|--arg-file)$s$v|$o)*"
  via+="|mise$s(exec|x)$s((-C|--cd|-E|--env|-j|--jobs)$s$v|$o|[^[:space:];&|@]+@[^[:space:];&|]+$s)*"
  via+="|\"?([^[:space:];&|\"]*/)?capped\"?$s($o)*))"
  cargo_rules "$code" ""
  while IFS= read -r script; do
    case "$script" in *.py | *.qml) continue ;; esac
    [ -f "${2:-$ljos_root}/$script" ] || continue
    cargo_rules "$(shell_code "${2:-$ljos_root}/$script")" "$script: "
  done <<EOF_SCRIPTS
$(grep -oE 'scripts/[A-Za-z0-9_.-]+' <<<"$code" | sort -u)
EOF_SCRIPTS
  return 0
}

# cargo_rules <code> <who>: cargo_problems's word on one shell program's
# code, each problem led by <who> ("<script>: " for a script, nothing
# for the task); pre and via are cargo_problems's.
cargo_rules() {
  local words
  words="$(command_words <<<"$1")"
  grep -Eq "${pre}${via}*cargo([[:space:]]|$)" <<<"$words" || return 0
  grep -Eq '^[[:space:]]*(\.|source)[[:space:]].*_lib/env\.sh"?[[:space:]]*$' <<<"$1" ||
    printf ' %sruns cargo without sourcing _lib/env.sh, so it builds into a worktree'"'"'s own target/;' "$2"
  grep -Eq "${pre}${via}+cargo([[:space:]]|$)" <<<"$words" &&
    printf ' %sexec cargo, command cargo or a program that runs cargo (timeout, nice, env, capped) goes round env.sh'"'"'s cargo (run cargo, or exec_cargo where cargo takes the process);' "$2"
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
# program the build made, one per line, decoded as json_string does. A path
# json_string leaves empty, or one with a newline in it, is no program.
built_programs() {
  local exe nl='
'
  while IFS= read -r exe; do
    exe="$(printf '%s\n' "$exe" | json_string)"
    case "$exe" in "" | *"$nl"*) continue ;; esac
    printf '%s\n' "$exe"
  done <<EOF_BUILT
$(sed -n -E 's/^\{"reason":"compiler-artifact".*"executable":"(([^"\\]|\\.)*)".*/\1/p')
EOF_BUILT
}

# pin_build <cargo args…>: cargo with its JSON messages on stdout; any other
# line it prints there goes to stderr.
pin_build() {
  local out
  out="$(cargo "$@" --message-format=json-render-diagnostics)" || return 1
  printf '%s\n' "$out" | grep -v -e '^{"reason":' -e '^$' >&2 || true
  printf '%s\n' "$out" | grep '^{"reason":' || true
}

# build_dir: where cargo keeps the intermediate files of this checkout's
# build (build.build-dir), as cargo itself decides it for a build run here:
# CARGO_BUILD_BUILD_DIR, then the settings (with a linked worktree's main
# checkout's, as `cargo` gives them), then the target directory; its
# {workspace-root}, {cargo-cache-home} and {workspace-path-hash} expanded
# by cargo. Asking cargo keeps its order, which a reading of the settings
# here would have to copy.
build_dir() {
  local out dir
  out="$(cargo metadata --no-deps --format-version 1 --offline --manifest-path "$ljos_root/Cargo.toml")" || {
    printf '%s\n' "$(task_name): cargo could not say where this checkout's build directory is (above)" >&2
    return 1
  }
  dir="$(printf '%s\n' "$out" | sed -n -E 's/.*"build_directory":"(([^"\\]|\\.)*)".*/\1/p' | json_string)"
  [ -n "$dir" ] || dir="$(printf '%s\n' "$out" | sed -n -E 's/.*"target_directory":"(([^"\\]|\\.)*)".*/\1/p' | json_string)"
  [ -n "$dir" ] || {
    printf '%s\n' "$(task_name): cargo metadata named no build directory" >&2
    return 1
  }
  printf '%s\n' "$dir"
}

# pin_programs <dir> <cargo args…>: builds, gives the run its own link to
# every program the build made, in <dir>, and prints "<name> <path>" for
# each. A target directory may be shared (a linked worktree's is its main
# checkout's), and another checkout's build while a long run goes on would
# put that checkout's programs where the run's were. So the build keeps its
# intermediate files in the build directory it always uses (build_dir:
# nothing is built again) and puts its programs in a target of <dir>'s own,
# <dir>/.target, which no other checkout's build writes to. Cargo holds the
# build directory's lock from the compile to the programs' copies, so no
# other build comes between them. A hard link keeps the file it was made to,
# as cargo and the linker put new files in place of old ones (a <dir> on
# another drive than the target gets a copy).
pin_programs() {
  local into="$1" build messages exe pinned=""
  shift
  mkdir -p "$into"
  build="$(build_dir)" || return 1
  messages="$(CARGO_BUILD_BUILD_DIR="$build" CARGO_TARGET_DIR="$into/.target" pin_build "$@")" || return 1
  while IFS= read -r exe; do
    [ -n "$exe" ] || continue
    ln -f "$exe" "$into/${exe##*/}" 2>/dev/null || cp -p "$exe" "$into/${exe##*/}" || return 1
    pinned="$pinned${exe##*/} $into/${exe##*/}
"
  done <<EOF_PROGRAMS
$(printf '%s\n' "$messages" | built_programs)
EOF_PROGRAMS
  printf '%s' "$pinned"
}

# pinned_program <name> <pin_programs output>: the path pinned for program
# <name>, all of it (a path may hold spaces); nothing when there is none.
pinned_program() {
  local line
  while IFS= read -r line; do
    case "$line" in "$1 "*) printf '%s\n' "${line#"$1 "}"; return 0 ;; esac
  done <<<"$2"
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
