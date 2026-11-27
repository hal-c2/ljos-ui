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
    find "$dir/bin" -mindepth 2 -maxdepth 2 -name ljos -printf '%h\0' | xargs -0 -rn1 basename
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
from datetime import datetime
root, out, names, *commits = sys.argv[1:]
# The commit file is "<commit> <ISO date with its offset> <subject>"; the
# dates are compared as instants, not as text, as two offsets may differ.
dated = []
for c in commits:
    line = open(os.path.join(out, "bin", c, "commit")).read().split(" ", 2)
    dated.append((datetime.fromisoformat(line[1]), c))
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
    local gesture=(--gesture scroll) started status=0
    [ "$report" = inbox ] && gesture=(--gesture session --load 2)
    mkdir -p "$dir/report/$report"
    started="$(date +%s)"
    "$ljos" bench --manifest "$dir/$report.json" --runtime "$runtimes" --profile desktop \
      --sway "$(value SWAYSOCK)" --wayland "$(value WAYLAND_DISPLAY)" \
      --app-env "DBUS_SESSION_BUS_ADDRESS=$(value DBUS_SESSION_BUS_ADDRESS)" \
      --app-env "XDG_CONFIG_HOME=$(value XDG_CONFIG_HOME)" --app-env "XDG_DATA_HOME=$(value XDG_DATA_HOME)" \
      --app-env "XDG_STATE_HOME=$(value XDG_STATE_HOME)" \
      "${gesture[@]}" ${defaults[@]+"${defaults[@]}"} \
      --note "ljos: this checkout at $(git -C "$root" rev-parse --short HEAD); at-<commit>: that commit's ljos; all cargo build --release" \
      --note "on the sandbox's headless sway (${LJOS_CUA_GPU:-pixman}), as Wayland clients" \
      --out "$dir/report/$report" ${opts[@]+"${opts[@]}"} || status=$?
    # A refusal (2) ends the run with the report written so far, which the
    # table marks as partial; a report older than this run is not it.
    case "$status" in
      0) printf 'complete\n' > "$dir/report/$report/status" ;;
      2)
        [ -f "$dir/report/$report/report.json" ] &&
          [ "$(stat -c %Y "$dir/report/$report/report.json")" -ge "$started" ] ||
          die "bench:history: the $report run was refused before it measured anything"
        printf 'partial: a refusal ended the run\n' > "$dir/report/$report/status"
        ;;
      *) die "bench:history: the $report run failed" ;;
    esac
  done
  history_reload "$ljos" "${commits[@]}"
}

# Hot reload, which `ljos bench` does not drive: each revision's
# `ljos run --watch` on a copy of the inbox, a label in TopBar.qml saved
# again and again, and the times its own log gives for each save
# ("reloaded in", and "shown … after the save", which runs from the file's
# mtime to the first frame painted after it). The revisions take turns, a
# round at a time, as the bench runs interleave.
history_reload() {
  local ljos="$1" sandbox
  shift
  need_python3
  sandbox="$("$root/scripts/cua-sandbox.sh" env)"
  SANDBOX_ENV="$sandbox" python3 - "$root" "$dir" "$ljos" "${BENCH_HISTORY_RELOADS:-10}" "${BENCH_HISTORY_ROUNDS:-3}" "$@" <<'PY' ||
import json, os, re, shutil, signal, subprocess, sys, time
from datetime import datetime
root, out, now, edits, rounds, *commits = sys.argv[1:]
edits, rounds = int(edits), int(rounds)
env = dict(os.environ)
for line in os.environ["SANDBOX_ENV"].splitlines():
    k, _, v = line.partition("=")
    if k:
        env[k] = v
dated = sorted((datetime.fromisoformat(open(os.path.join(out, "bin", c, "commit")).read().split(" ")[1]), c)
               for c in commits)
runtimes = [(f"at-{c[:9]}", os.path.join(out, "bin", c, "ljos")) for _, c in dated] + [("ljos", now)]

# One copy, put back as it was before every run; the records are read
# where the bench reads them.
src = os.path.join(root, "bench", "inbox")
copy = os.path.join(out, "reload", "inbox")
shutil.copytree(src, copy, dirs_exist_ok=True, ignore=shutil.ignore_patterns("data"))
if not os.path.lexists(os.path.join(copy, "data")):
    os.symlink(os.path.join(src, "data"), os.path.join(copy, "data"))
bar = os.path.join(copy, "TopBar.qml")
original = open(os.path.join(src, "TopBar.qml")).read()
if "t`Inbox`" not in original:
    sys.exit("bench:history: TopBar.qml no longer has the label the reloads change")

timed = re.compile(r"reloaded in ([\d.]+) ms \(compile ([\d.]+) ms, apply ([\d.]+) ms\)[^;\n]*(?:; shown ([\d.]+) ms after the save)?")
keys = ("reload_ms", "compile_ms", "apply_ms", "shown_ms")
results = {name: {**{k: [] for k in keys}, "saves": 0, "failed": 0, "exited": 0} for name, _ in runtimes}
logs = os.path.join(out, "reload", "logs")
os.makedirs(logs, exist_ok=True)
for r in range(rounds):
    for name, program in runtimes:
        open(bar, "w").write(original)
        log = os.path.join(logs, f"{name}-{r}.log")
        with open(log, "w") as f:
            p = subprocess.Popen([program, "run", "--watch", os.path.join(copy, "Main.qml"), "--size", "1280x800"],
                                 env=env, stdout=f, stderr=subprocess.STDOUT, start_new_session=True)
        # The records are read in the turns after the first frame; the
        # saves start once they are in.
        time.sleep(5)
        for i in range(edits):
            if p.poll() is not None:
                break
            open(bar, "w").write(original.replace("t`Inbox`", "t`Inbox`" if i % 2 else "t`Inbox ·`", 1))
            results[name]["saves"] += 1
            time.sleep(1.5)
        if p.poll() is None:
            os.killpg(p.pid, signal.SIGTERM)
            try:
                p.wait(10)
            except subprocess.TimeoutExpired:
                os.killpg(p.pid, signal.SIGKILL)
                p.wait()
        else:
            results[name]["exited"] += 1
        for text in open(log, errors="replace"):
            m = timed.search(text)
            if m:
                for k, g in zip(keys, m.groups()):
                    if g:
                        results[name][k].append(float(g))
            elif "reload failed" in text:
                results[name]["failed"] += 1
        print(f"bench:history: reload {name}, round {r + 1}: {len(results[name]['reload_ms'])} reloads of {results[name]['saves']} saves", flush=True)
open(bar, "w").write(original)
os.makedirs(os.path.join(out, "report", "reload"), exist_ok=True)
json.dump({"order": [n for n, _ in runtimes], "runtimes": results},
          open(os.path.join(out, "report", "reload", "reload.json"), "w"), indent=1)
PY
    die "bench:history: the reloads were not measured"
}

# history.md: for each app and metric, the median over the repeats of
# every revision, oldest first, with the spread (min–max) beside it and
# the change from the column before.
history_table() {
  need_python3
  python3 - "$dir" <<'PY'
import json, os, statistics, sys
from datetime import datetime
out = sys.argv[1]
runs, partial = [], []
for r in ("apps", "inbox"):
    path = os.path.join(out, "report", r, "report.json")
    if os.path.exists(path):
        runs += json.load(open(path))["runs"]
        status = os.path.join(out, "report", r, "status")
        said = open(status).read().strip() if os.path.exists(status) else "complete"
        if said != "complete":
            partial.append(f"{r}: {said}")
if not runs:
    sys.exit(f"bench:history table: no report in {out}/report; run bench:history measure first")

# Each revision's runtime is at-<the first 9 of its commit>; its commit
# file says when it was made.
dates = {}
for d in os.listdir(os.path.join(out, "bin")):
    dates[f"at-{d[:9]}"] = datetime.fromisoformat(open(os.path.join(out, "bin", d, "commit")).read().split(" ")[1])
never = datetime.min.replace(tzinfo=datetime.now().astimezone().tzinfo)
order = sorted({r["runtime"] for r in runs if r["runtime"] != "ljos"}, key=lambda n: dates.get(n, never))
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
lines += [f"**Partial**: {p}." for p in partial] + ([""] if partial else [])
for app in sorted({r["app"] for r in runs}):
    mine = [r for r in runs if r["app"] == app]
    lines += [f"## {app}", "", "| metric | " + " | ".join(order) + " |", "|---" * (len(order) + 1) + "|"]
    # How every run ended, so a revision whose runs all failed still has
    # its column filled.
    def ended(rt):
        counts = {}
        for r in mine:
            if r["runtime"] == rt:
                counts[r.get("outcome")] = counts.get(r.get("outcome"), 0) + 1
        return ", ".join(f"{n} {o}" for o, n in sorted(counts.items(), key=lambda x: str(x[0]))) or "—"
    lines.append("| runs | " + " | ".join(ended(rt) for rt in order) + " |")
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
reload = os.path.join(out, "report", "reload", "reload.json")
if os.path.exists(reload):
    got = json.load(open(reload))["runtimes"]
    cols = [rt for rt in order if rt in got]
    lines += ["## inbox, hot reload", "",
              "A label in TopBar.qml saved again and again under `ljos run --watch`; the times are the run's own log.", "",
              "| metric | " + " | ".join(cols) + " |", "|---" * (len(cols) + 1) + "|"]
    for label, key in (("reload, ms", "reload_ms"), ("compile, ms", "compile_ms"), ("apply, ms", "apply_ms"),
                       ("shown after the save, ms", "shown_ms")):
        cells = []
        for rt in cols:
            vals = got[rt][key]
            cells.append(f"{fmt(statistics.median(vals))}<br>{fmt(min(vals))}–{fmt(max(vals))} ({len(vals)})" if vals else "—")
        lines.append(f"| {label} | " + " | ".join(cells) + " |")
    lines.append("| saves, failed reloads, exits | " + " | ".join(
        f"{got[rt]['saves']}, {got[rt]['failed']}, {got[rt]['exited']}" for rt in cols) + " |")
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
