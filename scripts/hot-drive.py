#!/usr/bin/env python3
"""Edit corpus examples while they run under `ljos run --watch`, off the user's screen, and say
what each save kept, lost and logged, and how long it took to show.

hot-drive.py <out dir> <id>...

Each example is copied to <out>/<id>/src (the corpus is never written) and runs in the
desktop:cua sandbox with the inspector on. It is put in a state that is not its first one: text
typed into its first field, its first list scrolled, a control clicked (a tab chosen, a popup
opened). Then the copy is saved with one edit after another, through the file watcher as an editor
would save it:

    colour       the first colour literal of the entry file
    text         the first `text: "…"` of the entry file
    add          an item added to the root
    remove       that item taken away again
    binding      the first bound `width`/`height`/`spacing`/`radius` of the entry file, plus one
    sibling      a colour or text of another file of the example
    break        a syntax error in the entry file
    fix          the syntax error taken out again

After each save the driver waits for the watcher's line on stderr (a save that puts back what runs
makes no new generation, so there is nothing else to wait for), snapshots the outline and compares
it with the one before the save. <out>/<id>/report.txt has every step; <out>/
summary.md has a table: per example and edit, what the watcher said (with the time from the save
to the frame that shows it), whether the window and the state made before (the typed text, the
scroll position, what the click opened) were still there, and what was logged."""
import importlib.util, json, os, re, shutil, subprocess, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
spec = importlib.util.spec_from_file_location("gallery_drive", os.path.join(ROOT, "scripts", "gallery-drive.py"))
gd = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gd)

TARGET, SANDBOX = gd.TARGET, gd.SANDBOX
PROFILE = os.environ.get("GALLERY_PROFILE", "debug")
# How long a save may take to show before the driver says it never did.
RELOAD_WAIT_MS = 20000
STARTUP = 60
COLOUR = re.compile(r'"#[0-9a-fA-F]{6}"')
TEXT = re.compile(r'(\btext:\s*(?:qsTr\()?")([^"\\\n]+)(")')
BINDING = re.compile(r'^(\s*(?:width|height|spacing|radius):\s*)([^\n;{}]*[A-Za-z_][^\n;{}]*?)\s*$', re.M)
QTQUICK = re.compile(r'^import QtQuick(\s+[\d.]+)?\s*$', re.M)
ADDED = 'Item { objectName: "hotDriveAdded"; width: 1; height: 1 }'
# How the watcher's line for a save starts, on stderr.
WATCHER = ("ljos: reloaded", "ljos: reload failed", "ljos: the files are what runs again")
# An outline line of a scroll bar, with where it is.
SCROLLED = re.compile(r'^[ \t]*- (scrollbar)[ \t]+value=(\S+)[ \t]', re.M)
# A line no QML file can hold.
BROKEN = "property int : 3"


def examples():
    return {e["id"]: e for e in json.load(open(os.path.join(ROOT, "corpus", "examples.json")))["examples"]}


def root_body(text):
    """Where the body of the file's root object starts: just after its `{`."""
    for m in re.finditer(r'^[A-Z][\w.]*\s*\{', text, re.M):
        return m.end()
    return None


def save(path, text):
    # As an editor saves: the whole file written and closed.
    with open(path, "w") as f:
        f.write(text)


def edit_once(path, pattern, replace):
    text = open(path).read()
    m = pattern.search(text)
    if not m:
        return None
    save(path, text[:m.start()] + replace(m) + text[m.end():])
    return text[m.start():m.end()].strip()


def edits(src, entry):
    """The edits, in order: (name, apply) where apply saves the files and says what it changed,
    or None if the example has nothing to edit that way."""
    main = os.path.join(src, entry)
    others = sorted(
        (p for p in _qml_files(src) if os.path.abspath(p) != os.path.abspath(main)),
        key=lambda p: -os.path.getsize(p),
    )

    def colour():
        return edit_once(main, COLOUR, lambda m: '"#ff00ff"')

    def text():
        return edit_once(main, TEXT, lambda m: m.group(1) + m.group(2) + " (edited)" + m.group(3))

    def add():
        t = open(main).read()
        at = root_body(t)
        # `Item` is QtQuick's: a file that imports only its own modules has no name for it.
        if at is None or not QTQUICK.search(t):
            return None
        save(main, t[:at] + "\n    " + ADDED + "\n" + t[at:])
        return ADDED

    def remove():
        t = open(main).read()
        if ADDED not in t:
            return None
        save(main, t.replace("\n    " + ADDED + "\n", "", 1))
        return ADDED

    def binding():
        return edit_once(main, BINDING, lambda m: m.group(1) + "(" + m.group(2) + ") + 1")

    def sibling():
        for p in others:
            for pat, rep in ((COLOUR, lambda m: '"#ff00ff"'),
                             (TEXT, lambda m: m.group(1) + m.group(2) + " (edited)" + m.group(3))):
                did = edit_once(p, pat, rep)
                if did:
                    return os.path.relpath(p, src) + ": " + did
        return None

    def broken():
        t = open(main).read()
        at = root_body(t)
        if at is None:
            return None
        save(main, t[:at] + "\n    " + BROKEN + "\n" + t[at:])
        return BROKEN

    def fix():
        t = open(main).read()
        if BROKEN not in t:
            return None
        save(main, t.replace("\n    " + BROKEN + "\n", "", 1))
        return "the line taken out"

    return [("colour", colour), ("text", text), ("add", add), ("remove", remove),
            ("binding", binding), ("sibling", sibling), ("break", broken), ("fix", fix)]


def _qml_files(src):
    for d, _, files in os.walk(src):
        for f in files:
            if f.endswith(".qml"):
                yield os.path.join(d, f)


def status(app, **args):
    r = app.rpc("tools/call", {"name": "app_status", "arguments": args})
    return (r.get("result") or {}).get("structuredContent") or {}


def generation(st):
    reload = st.get("reload") or {}
    return reload.get("generation", 0) if isinstance(reload, dict) else 0


def outline(app):
    """The outline without the line that numbers the snapshot."""
    return "\n".join(l for l in app.call("app_snapshot").splitlines() if not l.startswith("(snapshot"))


def put_in_state(app, w):
    """Makes the state a reload should keep. Returns what to look for after each save:
    (label, regex over the outline)."""
    marks = []
    snap = app.call("app_snapshot")
    m = gd.FIELD.search(snap)
    if m:
        ref = m.group(3)
        res = app.call("app_press", {"target": ref, "keys": "Ctrl+A Backspace"})
        res = app.call("app_type", {"text": "kept text"})
        w(f"== typed into {m.group(1)} [{ref}]", res.splitlines()[0] if res else "")
        marks.append(("typed text", re.compile(r'value="[^"]*kept text')))
    before = app.call("app_snapshot")
    res = app.call("app_scroll", {"dy": -3})
    w("== scrolled", res.splitlines()[0] if res else "")
    # A flick goes on moving after the wheel: the outline is read once it has stopped.
    after, end = outline(app), time.monotonic() + 2
    while time.monotonic() < end:
        time.sleep(0.2)
        again = outline(app)
        if again == after:
            break
        after = again
    if after != before:
        # What the scroll brought into view, as the outline names it.
        gone = set(after.splitlines()) - set(before.splitlines())
        lines = [l for l in gone if re.search(r'"[^"]{3,}"', l)]
        if lines:
            name = re.search(r'"([^"]{3,})"', lines[0]).group(1)
            marks.append(("scrolled to " + name[:30], re.compile(re.escape('"' + name + '"'))))
        else:
            # Nothing named came into view (a surface of pictures): the scroll bar that moved is the mark.
            moved = [m for m in map(SCROLLED.search, sorted(gone)) if m]
            if moved:
                role, value = moved[0].groups()
                marks.append((f"{role} scrolled to {value}",
                              re.compile(rf'^[ \t]*- {role}[ \t]+value={re.escape(value)}[ \t]', re.M)))
    for c in gd.CONTROL.finditer(after):
        name, ref = c.group(2) or "", c.group(3)
        if gd.ENDS.search(name) or "disabled" in c.group(4):
            continue
        prev = app.call("app_snapshot")
        res = app.call("app_click", {"target": ref})
        app.settle(0)
        now = app.call("app_snapshot")
        new = [l for l in set(now.splitlines()) - set(prev.splitlines()) if re.search(r'"[^"]{3,}"', l)]
        w(f"== clicked {name!r} [{ref}]", res.splitlines()[0] if res else "")
        if new:
            shown = re.search(r'"([^"]{3,})"', sorted(new)[0]).group(1)
            marks.append((f"after clicking {name[:20]!r}: {shown[:30]}", re.compile(re.escape('"' + shown + '"'))))
            break
    return marks


def stderr_lines(log, seen):
    try:
        lines = open(log, errors="replace").read().splitlines()
    except OSError:
        return []
    new = lines[seen[0]:]
    seen[0] = len(lines)
    return [l for l in new if l.strip()]


def drive(out, e):
    eid = e["id"]
    d = os.path.join(out, eid)
    shutil.rmtree(d, ignore_errors=True)
    src = os.path.join(d, "src")
    shutil.copytree(os.path.join(ROOT, "corpus", "qtdoc", e["dir"]), src)
    shutil.rmtree(gd.XDG, ignore_errors=True)
    gd.seed_pictures()
    win = e.get("window") or {}
    size = f"{win.get('width', 800)}x{win.get('height', 600)}"
    log_path = os.path.join(d, "stderr.log")
    log = open(log_path, "w")
    before = set(gd.glob.glob(TARGET + "/ljos/mcp-*.json"))
    cmd = [SANDBOX, "exec", os.path.join(TARGET, PROFILE, "ljos"), "run", os.path.join(src, e["entry"]),
           "--project", src, "--size", size, "--example", eid, "--watch", "--mcp=inspect,input,eval"]
    p = subprocess.Popen(cmd, cwd=ROOT, stdout=log, stderr=log, start_new_session=True)
    rep = open(os.path.join(d, "report.txt"), "w")

    def w(*lines):
        for l in lines:
            print(l, file=rep, flush=True)

    rows, path = [], None
    try:
        for _ in range(STARTUP * 2):
            time.sleep(0.5)
            new = set(gd.glob.glob(TARGET + "/ljos/mcp-*.json")) - before
            if new:
                path = new.pop()
                break
            if p.poll() is not None:
                break
        if not path:
            return {"id": eid, "started": False, "rows": []}
        time.sleep(2)
        app = gd.Inspector(path)
        seen = [0]
        w("== logged while starting", *app.new_logs(), *stderr_lines(log_path, seen))
        marks = put_in_state(app, w)
        w("== state to keep: " + "; ".join(l for l, _ in marks), *app.new_logs())
        stderr_lines(log_path, seen)
        for name, apply in edits(src, e["entry"]):
            gen = generation(status(app))
            prev = app.call("app_snapshot")
            did = apply()
            if did is None:
                rows.append({"edit": name, "skipped": True})
                w(f"== {name}: nothing to edit")
                continue
            t0 = time.monotonic()
            said = []
            # The watcher's line comes with the frame that shows the result (or says it failed).
            while time.monotonic() - t0 < RELOAD_WAIT_MS / 1000 and p.poll() is None:
                said += stderr_lines(log_path, seen)
                if any(l.startswith(WATCHER) for l in said):
                    break
                time.sleep(0.05)
            waited = (time.monotonic() - t0) * 1000
            time.sleep(0.3)
            said += stderr_lines(log_path, seen)
            said = [re.sub(r"^ljos: ", "", l) for l in said]
            alive = p.poll() is None
            st = status(app) if alive else {}
            logs = app.new_logs() if alive else []
            now = app.call("app_snapshot") if alive else ""
            windows = len(st.get("windows") or []) if alive else 0
            kept = [l for l, rx in marks if rx.search(now)]
            lost = [l for l, rx in marks if rx.search(prev) and not rx.search(now)]
            changed = len(set(now.splitlines()) ^ set(prev.splitlines()))
            row = {"edit": name, "did": did, "alive": alive, "windows": windows,
                   "generation": generation(st), "was": gen, "waited_ms": round(waited),
                   "said": said, "logs": logs, "kept": kept, "lost": lost, "changed_lines": changed}
            rows.append(row)
            w(f"== {name}: {did}", json.dumps({k: v for k, v in row.items() if k not in ("said", "logs")}),
              "-- watcher:", *said, "-- app log:", *logs)
            if not alive:
                break
        return {"id": eid, "started": True, "rows": rows}
    except Exception as x:
        w(f"== driver error {x!r}")
        rows.append({"edit": "driver", "did": repr(x), "alive": p.poll() is None, "windows": 0,
                     "said": [repr(x)[:160]], "logs": [], "kept": [], "lost": []})
        return {"id": eid, "started": True, "rows": rows}
    finally:
        if p.poll() is None:
            os.killpg(p.pid, 15)
            time.sleep(1)


def watcher_says(said):
    for l in said:
        m = re.search(r"reloaded in ([\d.]+) ms \(compile ([\d.]+) ms, apply ([\d.]+) ms\): (\d+) kept, (\d+) new, (\d+) made again(?:; shown ([\d.]+) ms)?", l)
        if m:
            shown = f", shown {float(m.group(7)):.0f} ms" if m.group(7) else ""
            return f"reloaded: compile {float(m.group(2)):.0f} + apply {float(m.group(3)):.0f} ms{shown}; {m.group(4)} kept, {m.group(5)} new, {m.group(6)} made again"
        if l.startswith("reload failed"):
            return l[:160]
        if l.startswith("the files are what runs again"):
            return l
    return "nothing"


def main():
    out, ids = sys.argv[1], sys.argv[2:]
    os.makedirs(out, exist_ok=True)
    known = examples()
    with open(os.path.join(out, "summary.md"), "w") as s:
        print("| example | edit | watcher | window | state kept | state lost | logged |", file=s)
        print("|---|---|---|---|---|---|---|", file=s)
        for eid in ids:
            r = drive(out, known[eid])
            if not r["started"]:
                print(f"| {eid} | | did not start | | | | |", file=s, flush=True)
                continue
            for row in r["rows"]:
                if row.get("skipped"):
                    continue
                warn = [l for l in row["said"] if l.startswith(("warning", "error", "  "))] + row["logs"]
                print(f"| {eid} | {row['edit']} | {watcher_says(row['said'])} | "
                      f"{'yes' if row['alive'] and row['windows'] else 'NO'} | {', '.join(row['kept']) or '-'} | "
                      f"{', '.join(row['lost']) or '-'} | {len(warn)}: {' / '.join(w[:80] for w in warn[:2])} |",
                      file=s, flush=True)
            print(eid, "done", flush=True)


if __name__ == "__main__":
    main()
