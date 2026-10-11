#!/usr/bin/env bash
# overlay.sh <example dir> <overlay dir>: a qmldir for every module a CMakeLists.txt registers with qt_add_qml_module
set -euo pipefail
src=$1; out=$2
while IFS= read -r cm; do
  dir=$(dirname "$cm")
  # one call may register several modules; take the text of each call
  awk -v dir="$dir" -v out="$out" '
    function flush(   i, path, name, f, base) {
      if (uri == "") return
      path = uri; gsub(/\./, "/", path)
      system("mkdir -p \"" out "/" path "\"")
      qd = out "/" path "/qmldir"
      cmd = "realpath -m --relative-to=\"" out "/" path "\" \"" dir "\""
      cmd | getline rel; close(cmd)
      print "module " uri > qd
      for (i = 1; i <= n; i++) {
        f = files[i]; base = f; sub(/.*\//, "", base)
        name = base; sub(/(\.ui)?\.qml$/, "", name)
        if (name ~ /^[A-Z]/) {
          cmd = "grep -q \"pragma Singleton\" \"" dir "/" f "\""
          if (system(cmd) == 0) print "singleton " name " 1.0 " rel "/" f > qd
          else print name " 1.0 " rel "/" f > qd
        }
      }
      close(qd)
      uri = ""; n = 0
    }
    /qt_add_qml_module|qt6_add_qml_module/ { flush(); incall = 1; depth = 0 }
    incall {
      line = $0
      if (match(line, /URI[ \t]+"?[A-Za-z0-9_.]+"?/)) {
        u = substr(line, RSTART + 3, RLENGTH - 3); gsub(/[ \t"]/, "", u); uri = u
      }
      m = split(line, toks, /[ \t()]+/)
      for (k = 1; k <= m; k++) if (toks[k] ~ /\.qml$/) files[++n] = toks[k]
      o = gsub(/\(/, "(", line); c = gsub(/\)/, ")", line)
      depth += o - c
      if (depth <= 0 && (o + c) > 0) { flush(); incall = 0 }
    }
    END { flush() }
  ' "$cm"
done < <(find "$src" -name CMakeLists.txt)
