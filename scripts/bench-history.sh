# shellcheck shell=bash disable=SC2154
# The measuring half of `mise run bench:history`, sourced by it with
# root, dir, revs and opts set and mise-tasks/_lib/env.sh loaded.
#
# Every built revision is a runtime of one `ljos bench` run, through the
# manifest's "commands", next to this checkout's own `ljos`: the same
# harness measures them all, interleaved, so a regression is one column
# against its neighbours rather than one afternoon against another.
#
# Two runs, as a run has one gesture: the bench apps under the wheel
# (startup, memory, idle CPU, scroll frames), and the inbox under its
# scripted session (search and edit latency, the PageDown scroll).

history_revisions() {
  local commit
  if [ ${#revs[@]} -gt 0 ]; then
    for rev in "${revs[@]}"; do
      commit="$(commit_of "$rev")"
      [ -x "$dir/bin/$commit/ljos" ] || die "bench:history: $rev is not built (bench:history build $rev)"
      printf '%s\n' "$commit"
    done
  else
    [ -d "$dir/bin" ] || die "bench:history: nothing is built in $dir/bin (bench:history build <rev…>)"
    find "$dir/bin" -mindepth 2 -maxdepth 2 -name ljos -printf '%h\n' | xargs -rn1 basename
  fi
}

history_measure() {
  local programs ljos sandbox commits runtimes report
  need_python3
  need_systemd_run
  programs="$(pin_programs "$dir/now" build --quiet --release -p ljos_cli --bin ljos)" || die "bench:history: ljos did not build"
  ljos="$(pinned_program ljos "$programs")"
  [ -n "$ljos" ] || die "bench:history: the build made no ljos"
  local listed
  listed="$(history_revisions)" || exit 1
  mapfile -t commits <<< "$listed"
  [ ${#commits[@]} -gt 0 ] || die "bench:history: no revision to measure"

  # The inbox reads 10 000 generated records; they are made once.
  python3 "$root/bench/inbox/fixture/generate.py" --check "$root/bench/inbox/data" >/dev/null 2>&1 ||
    python3 "$root/bench/inbox/fixture/generate.py" "$root/bench/inbox/data"

  # Oldest first, so a column's left neighbour is the commit before it.
  runtimes="$(HISTORY_DIR="$dir" python3 - "$root" "$dir" "${BENCH_HISTORY_APPS:-minimal feed list1000}" "${commits[@]}" <<'PY'
import json, os, re, sys
root, out, names, *commits = sys.argv[1:]
dated = []
for c in commits:
    line = open(os.path.join(out, "bin", c, "commit")).read().split(" ", 2)
    dated.append((line[1], c))
dated.sort()
commands = {f"at-{c[:9]}": os.path.join(out, "bin", c, "ljos") for _, c in dated}

def app(id, path, extra=None):
    text = open(path).read()
    a = {"id": id, "file": path, "imports": [os.path.dirname(path)], "qt": False}
    w = re.search(r"\bwidth:\s*(\d+)", text)
    h = re.search(r"\bheight:\s*(\d+)", text)
    if w and h:
        a["size"] = f"{w.group(1)}x{h.group(1)}"
    a.update(extra or {})
    args = ["run", a["file"]] + (["--size", a["size"]] if "size" in a else [])
    a["commands"] = {name: [program, *args] for name, program in commands.items()}
    return a

apps = [app(n, os.path.join(root, "bench", f"{n}.qml")) for n in names.split()]
json.dump({"apps": apps}, open(os.path.join(out, "apps.json"), "w"), indent=1)
inbox = app("inbox", os.path.join(root, "bench", "inbox", "Main.qml"),
            {"session": json.load(open(os.path.join(root, "bench", "inbox", "session.json")))})
json.dump({"apps": [inbox]}, open(os.path.join(out, "inbox.json"), "w"), indent=1)
print(",".join([*commands, "ljos"]))
PY
)" || die "bench:history: the manifests were not written"

  sandbox="$("$root/scripts/cua-sandbox.sh" env)"
  value() { sed -n "s/^$1=//p" <<< "$sandbox"; }
  read -r -a defaults <<< "${BENCH_DEFAULTS---warm-up --repeat 3}"
  for report in apps inbox; do
    local gesture=(--gesture scroll)
    [ "$report" = inbox ] && gesture=(--gesture session --load 2)
    "$ljos" bench --manifest "$dir/$report.json" --runtime "$runtimes" --profile desktop \
      --sway "$(value SWAYSOCK)" --wayland "$(value WAYLAND_DISPLAY)" \
      --app-env "DBUS_SESSION_BUS_ADDRESS=$(value DBUS_SESSION_BUS_ADDRESS)" \
      --app-env "XDG_CONFIG_HOME=$(value XDG_CONFIG_HOME)" --app-env "XDG_DATA_HOME=$(value XDG_DATA_HOME)" \
      --app-env "XDG_STATE_HOME=$(value XDG_STATE_HOME)" \
      "${gesture[@]}" ${defaults[@]+"${defaults[@]}"} \
      --note "ljos: this checkout at $(git -C "$root" rev-parse --short HEAD); at-<commit>: that commit's ljos; all cargo build --release" \
      --note "on the sandbox's headless sway (${LJOS_CUA_GPU:-pixman}), as Wayland clients" \
      --out "$dir/report/$report" ${opts[@]+"${opts[@]}"} ||
      [ $? -eq 2 ] || die "bench:history: the $report run failed"
  done
}

# history.md: for each app and metric, the median over the repeats of
# every revision, oldest first, with the spread (min–max) beside it and
# the change from the column before.
history_table() {
  need_python3
  python3 - "$dir" <<'PY'
import json, os, statistics, sys
out = sys.argv[1]
reports = [os.path.join(out, "report", r, "report.json") for r in ("apps", "inbox")]
runs = []
for path in reports:
    if os.path.exists(path):
        runs += json.load(open(path))["runs"]
if not runs:
    sys.exit(f"bench:history table: no report in {out}/report; run bench:history measure first")

# Each revision's runtime is at-<the first 9 of its commit>; its commit
# file says when it was made.
dates = {}
for d in os.listdir(os.path.join(out, "bin")):
    dates[f"at-{d[:9]}"] = open(os.path.join(out, "bin", d, "commit")).read().split(" ")[1]
order = sorted({r["runtime"] for r in runs if r["runtime"] != "ljos"}, key=lambda n: dates.get(n, ""))
order.append("ljos")

def settled(r, key):
    s = r.get("idled") or r.get("settled") or {}
    return s.get(key, (s.get("resident") or {}).get(key))

def step(name, key):
    def get(r):
        for s in r.get("steps") or []:
            if s.get("name") == name:
                return s.get(key)
    return get

metrics = [
    ("first frame, ms", lambda r: r.get("first_frame_ms")),
    ("first answering frame, ms", lambda r: r.get("input_ready_ms")),
    ("startup CPU, ms", lambda r: r.get("startup_cpu_ms")),
    ("PSS idle, MiB", lambda r: (settled(r, "pss") or 0) / 1048576 or None),
    ("RSS idle, MiB", lambda r: (settled(r, "rss") or 0) / 1048576 or None),
    ("idle CPU, %", lambda r: (r.get("idle") or {}).get("cpu_percent")),
    ("idle wakeups/s", lambda r: (r.get("idle") or {}).get("wakeups_per_s")),
    ("load CPU, %", lambda r: (r.get("load") or {}).get("cpu_percent")),
    ("load frames p95, ms", lambda r: ((r.get("load") or {}).get("frames") or {}).get("p95_ms")),
    ("load frames max, ms", lambda r: ((r.get("load") or {}).get("frames") or {}).get("max_ms")),
    ("select lag, ms", step("select row 0", "lag_ms")),
    ("2000 rows p95, ms", step("2000 rows down", "p95_ms")),
    ("2000 rows long frames", step("2000 rows down", "long")),
    ("search settle, ms", step("search", "settle_ms")),
    ("search lag, ms", step("search", "lag_ms")),
    ("title typing lag, ms", step("title typed", "lag_ms")),
    ("save settle, ms", step("save", "settle_ms")),
    ("theme settle, ms", step("theme", "settle_ms")),
    ("japanese settle, ms", step("japanese", "settle_ms")),
]

def fmt(x):
    return "—" if x is None else (f"{x:.3g}" if abs(x) < 100 else f"{x:.0f}")

lines = ["# Ljós against its own past", "",
         "Medians of the repeats, the spread (min–max) under them; "
         "each column is one commit's `ljos`, oldest first, `ljos` is this checkout.", ""]
for app in sorted({r["app"] for r in runs}):
    mine = [r for r in runs if r["app"] == app]
    lines += [f"## {app}", "", "| metric | " + " | ".join(order) + " |", "|---" * (len(order) + 1) + "|"]
    for label, get in metrics:
        cells, seen = [], False
        for rt in order:
            vals = [v for v in (get(r) for r in mine if r["runtime"] == rt and r.get("outcome") == "ok") if v is not None]
            if vals:
                seen = True
                m = statistics.median(vals)
                cells.append(f"{fmt(m)}<br>{fmt(min(vals))}–{fmt(max(vals))} ({len(vals)})")
            else:
                bad = {r.get("outcome") for r in mine if r["runtime"] == rt} - {"ok"}
                cells.append(", ".join(sorted(bad)) or "—")
        if seen:
            lines.append(f"| {label} | " + " | ".join(cells) + " |")
    lines.append("")
lines += ["## Revisions", ""]
for d in sorted(os.listdir(os.path.join(out, "bin")), key=lambda d: dates[f"at-{d[:9]}"]):
    c = open(os.path.join(out, "bin", d, "commit")).read().strip()
    lines.append(f"- `at-{d[:9]}`: {c}")
path = os.path.join(out, "history.md")
open(path, "w").write("\n".join(lines) + "\n")
print(path)
PY
}
