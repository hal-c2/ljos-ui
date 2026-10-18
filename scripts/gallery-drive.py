#!/usr/bin/env python3
"""Use each corpus example the way a person would, off the user's screen, and say what broke.

gallery-drive.py <out dir> <id>...

Each example runs in the desktop:cua sandbox with the inspector on (`gallery:run <id>
--mcp=inspect,input`). Every enabled control the snapshot shows is clicked once, those that look
like they end the app last. After each click the driver waits for the animations it started (a
page sliding in) and snapshots again, since a click may open a page with new controls. An example
with almost no controls (a game or a snippet built from MouseAreas) gets real pointer clicks
through cua-driver on a grid instead. What each click did (the outline's changes, or why the
inspector refused it) and what the app logged go in <out>/<id>/report.txt, with screenshots;
<out>/summary.txt lists, per example, whether it started, what its clicks did and the distinct
lines it logged."""
import base64, glob, json, os, re, shutil, signal, subprocess, sys, time, urllib.error, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target")
CORPUS = "file://" + os.path.join(ROOT, "corpus", "qtdoc") + "/"
SANDBOX = os.path.join(ROOT, "scripts", "cua-sandbox.sh")
# The sandbox's config, data and state directories; cua-sandbox.sh makes them again, with the
# user's fonts, for each command it runs.
XDG = os.path.join(os.environ.get("LJOS_CUA_HOME") or os.path.join(TARGET, "cua"), "xdg")
ROLES = r"button|checkbox|switch|radio|radiobutton|tab|menuitem|link|combobox|spinbox|slider"
# An outline line of a control: its role, its name if it has one, what else is shown before its
# reference (a slider's value), the reference and its states.
CONTROL = re.compile(r'^[ \t]*- (?:' + ROLES + r')[ \t]+("([^"]*)"[ \t]+)?(?:[^\s\[]\S*[ \t]+)*?\[(e\d+)\]([^\n]*)', re.M)
STARTUP = 120
MAX_CLICKS = 40
GRID = 5
# A control whose name or id says it ends the app.
ENDS = re.compile(r"exit|quit|close|power|shut ?down", re.I)
# How long a click may keep the app moving (a page sliding in) before the driver looks again.
SETTLE = 3.0


class Inspector:
    """The app's own MCP inspector, over HTTP JSON-RPC."""

    def __init__(self, path):
        info = json.load(open(path))
        self.pid, self.url, self.token, self.sid, self.n = info["pid"], info["url"], info["token"], None, 0
        self.since = None
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                "clientInfo": {"name": "gallery-drive", "version": "0"}})

    def rpc(self, method, params):
        self.n += 1
        headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream",
                   "Authorization": "Bearer " + self.token}
        if self.sid:
            headers["Mcp-Session-Id"] = self.sid
        body = json.dumps({"jsonrpc": "2.0", "id": self.n, "method": method, "params": params}).encode()
        for _ in range(30):
            try:
                r = urllib.request.urlopen(urllib.request.Request(self.url, body, headers), timeout=60)
                break
            except urllib.error.HTTPError as e:
                # The inspector limits how fast a client may call it.
                if e.code != 429:
                    raise
                time.sleep(float(e.headers.get("Retry-After") or 1))
        else:
            raise RuntimeError("the inspector kept answering 429")
        self.sid = r.headers.get("Mcp-Session-Id") or self.sid
        text = r.read().decode()
        if text.lstrip().startswith(("event:", "data:")):
            text = "\n".join(l[5:] for l in text.splitlines() if l.startswith("data:"))
        return json.loads(text)

    def call(self, tool, args=None, shot=None):
        r = self.rpc("tools/call", {"name": tool, "arguments": args or {}})
        res, out = r.get("result") or {}, []
        for c in res.get("content", []):
            if c.get("type") == "image" and shot:
                open(shot, "wb").write(base64.b64decode(c["data"]))
            elif c.get("type") == "text":
                out.append(c["text"])
        if "error" in r:
            out.append("RPC ERROR " + json.dumps(r["error"]))
        if res.get("isError"):
            out.insert(0, "TOOL ERROR")
        return "\n".join(out)

    def ended(self):
        """Whether the app's process is gone, waiting a little for it to finish ending."""
        for _ in range(20):
            try:
                os.kill(self.pid, 0)
            except ProcessLookupError:
                return True
            time.sleep(0.25)
        return False

    def animations(self):
        r = self.rpc("tools/call", {"name": "app_status", "arguments": {}})
        return ((r.get("result") or {}).get("structuredContent") or {}).get("animations", 0)

    def settle(self, before):
        """Waits until the animations a click started have ended: down to as many as ran before
        it (a spinner that runs for ever is not waited for). False: they were still running."""
        end = time.monotonic() + SETTLE
        while self.animations() > before:
            if time.monotonic() > end:
                return False
            time.sleep(0.1)
        return True

    def new_logs(self):
        """The lines logged since the last call, without the paging note."""
        text = self.call("app_logs", {} if self.since is None else {"since": self.since})
        m = re.search(r"since: (\d+)", text)
        if m:
            self.since = int(m.group(1))
        return [l for l in text.splitlines() if l.strip() and "since:" not in l and "no log lines" not in l]


def cua(tool, args):
    r = subprocess.run([SANDBOX, "call", tool, json.dumps(args)], capture_output=True, text=True, timeout=60)
    try:
        return json.loads(r.stdout)
    except ValueError:
        return {"error": (r.stdout + r.stderr)[-400:]}


def outcome(res, settled):
    """What a click did, in a few words: what changed in the outline, or why it was refused."""
    if res.startswith("TOOL ERROR"):
        m = re.search(r'"code":"(\w+)","message":"((?:[^"\\]|\\.)*)"', res)
        return f"refused, {m.group(1)}: {m.group(2)}" if m else "refused: " + res[10:200].strip()
    m = re.search(r"\(snapshot \d+: (\d+) changed, (\d+) added, (\d+) removed", res)
    if not m:
        return "clicked: " + res.splitlines()[0][:200]
    parts = [f"{n} {what}" for n, what in zip(map(int, m.groups()), ("changed", "added", "removed")) if n]
    did = ", ".join(parts) if parts else "nothing changed"
    return did if settled else did + f" (still moving after {SETTLE:g} s)"


def grid_clicks(app, w, d):
    wins = [x for x in cua("list_windows", {"pid": app.pid}).get("windows", []) if x["pid"] == app.pid]
    if not wins:
        w(f"== grid: no window for pid {app.pid}")
        return
    win = wins[0]
    # cua-driver reads x/y in the frame of the last window state it captured.
    cua("get_window_state", {"pid": app.pid, "window_id": win["window_id"], "include_accessibility_tree": False})
    for j in range(GRID):
        for i in range(GRID):
            x, y = int(win["width"] * (i + 0.5) / GRID), int(win["height"] * (j + 0.5) / GRID)
            r = cua("click", {"pid": app.pid, "window_id": win["window_id"], "x": x, "y": y})
            time.sleep(0.4)
            err = r.get("error") or r.get("code") or ""
            w(f"== grid click ({x},{y}) {err}".rstrip(), *app.new_logs())
    app.call("app_screenshot", {}, os.path.join(d, "grid.png"))


def why_it_ended(d, p):
    """What the example said when it ended before the driver could use it: Ljós's own lines
    (`ljos: setup: …`, a panic) from its stderr, and the exit status."""
    try:
        status = p.wait(timeout=10)
    except subprocess.TimeoutExpired:
        status = None
    text = re.sub(r"\x1b\[[0-9;]*m", "", open(os.path.join(d, "stderr.log"), errors="replace").read())
    own = [l for l in text.splitlines()
           if (l.startswith("ljos: ") and "inspector (" not in l) or "panicked at" in l]
    if not own:
        own = [l for l in text.splitlines() if re.search(r"\berror\b", l, re.I)][-3:]
    ended = f"it is still running, with no inspector after {STARTUP} s" if status is None else f"it ended with exit status {status}"
    return [l.replace(CORPUS, "") for l in own[:5]] + [ended]


def drive(out, eid):
    d = os.path.join(out, eid)
    os.makedirs(d, exist_ok=True)
    before = set(glob.glob(TARGET + "/ljos/mcp-*.json"))
    # What the last run saved (Settings, LocalStorage) would start this one somewhere else.
    shutil.rmtree(XDG, ignore_errors=True)
    log = open(os.path.join(d, "stderr.log"), "w")
    p = subprocess.Popen([SANDBOX, "exec", "mise", "run", "gallery:run", eid, "--mcp=inspect,input"],
                         cwd=ROOT, stdout=log, stderr=log, start_new_session=True)
    rep = open(os.path.join(d, "report.txt"), "w")
    said, clicks, outcomes = [], 0, {}

    def w(*lines):
        for l in lines:
            print(l, file=rep, flush=True)
            if not l.startswith("=="):
                said.append(re.sub(r"^\d+ ", "", l).replace(CORPUS, ""))

    # gallery:drive built Ljós before the first example (GALLERY_BUILT), so this is Ljós's own
    # start, which takes a second or two even with every core busy.
    path = None
    for _ in range(STARTUP * 2):
        time.sleep(0.5)
        new = set(glob.glob(TARGET + "/ljos/mcp-*.json")) - before
        if new:
            path = new.pop()
            break
        if p.poll() is not None:
            break
    try:
        if not path:
            return {"id": eid, "started": False, "why": why_it_ended(d, p)}
        # The inspector is up before the QML is made; an example that does not load ends here,
        # and its inspector file goes with it.
        for _ in range(8):
            time.sleep(0.25)
            if p.poll() is not None:
                break
        try:
            if p.poll() is not None:
                raise FileNotFoundError(path)
            app = Inspector(path)
        except (OSError, urllib.error.URLError, ValueError):
            if p.poll() is None and os.path.exists(path):
                raise
            return {"id": eid, "started": False, "why": why_it_ended(d, p)}
        print("== first snapshot\n" + app.call("app_snapshot"), file=rep, flush=True)
        app.call("app_screenshot", {}, os.path.join(d, "0-start.png"))
        w("== logged while starting", *app.new_logs())
        seen = set()
        while clicks < MAX_CLICKS:
            todo = []
            for m in CONTROL.finditer(app.call("app_snapshot")):
                ref, name, states = m.group(3), m.group(2) or "", m.group(4)
                if (name or ref) in seen:
                    continue
                # What the app does not let a person click is not clicked; it may be enabled
                # later, by another click, and is tried then.
                if re.search(r"(?:^|\s)disabled(?:\s|$)", states):
                    continue
                todo.append((ref, name, bool(ENDS.search(name + states))))
            if not todo:
                break
            # A control that may end the app goes last, so that it does not cut the drive short.
            todo.sort(key=lambda t: t[2])
            ref, name, _ = todo[0]
            seen.add(name or ref)
            clicks += 1
            try:
                before = app.animations()
                res = app.call("app_click", {"target": ref})
                settled = app.settle(before)
                logs = app.new_logs()
            except (OSError, urllib.error.URLError, ValueError):
                # A control may end the app (a power button calling Qt.quit()): that is
                # no hang, so it is told apart from an app that stopped answering.
                if not app.ended():
                    raise
                status = p.wait(timeout=10)
                w(f"== click {name!r} [{ref}]: the app ended (exit status {status}); see stderr.log")
                return {"id": eid, "started": True, "clicks": clicks, "outcomes": outcomes, "said": said,
                        "why": [f"the app ended after clicking {name or ref!r} (exit status {status})"]}
            did = outcome(res, settled)
            outcomes[did] = outcomes.get(did, 0) + 1
            w(f"== click {name!r} [{ref}]: {did}")
            # The outline's changes go in the report; only what the app logged is counted.
            for l in res.splitlines()[1:]:
                if not l.startswith("(snapshot"):
                    print("   " + l, file=rep, flush=True)
            w(*logs)
        if clicks < 3:
            grid_clicks(app, w, d)
        app.call("app_screenshot", {}, os.path.join(d, "end.png"))
        return {"id": eid, "started": True, "clicks": clicks, "outcomes": outcomes, "said": said}
    except Exception as x:
        w(f"== driver error {x!r}")
        return {"id": eid, "started": True, "clicks": clicks, "outcomes": outcomes, "said": said, "why": [repr(x)]}
    finally:
        if p.poll() is None:
            os.killpg(p.pid, signal.SIGTERM)
            time.sleep(1)


def stderr_warnings(out, eid):
    """Ljós's own warnings on stderr (the JS budget's, for one), which the inspector's log does
    not carry; without the time stamp and the time taken, so repeats count as one line."""
    try:
        text = open(os.path.join(out, eid, "stderr.log"), errors="replace").read()
    except OSError:
        return []
    text = re.sub(r"\x1b\[[0-9;]*m", "", text)
    lines = []
    for l in text.splitlines():
        m = re.match(r"\S+\s+WARN\s+(ljos\w*::.*)", l)
        if m:
            lines.append("stderr: " + re.sub(r"\s*elapsed=\S+", "", m.group(1)).replace(CORPUS, ""))
    return lines


def main():
    out, ids = sys.argv[1], sys.argv[2:]
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "summary.txt"), "w") as s:
        for eid in ids:
            r = drive(out, eid)
            r.setdefault("said", []).extend(stderr_warnings(out, eid))
            if not r["started"]:
                head = f"{eid}: did not start"
            else:
                head = f"{eid}: {r['clicks']} controls clicked"
            lines = [head] + ["    " + l for l in r.get("why", [])]
            # What the clicks did: each refusal with its reason, the rest by whether the
            # outline changed (report.txt has each click's changes).
            kinds = {}
            for did, n in r.get("outcomes", {}).items():
                kind = did if did.startswith(("refused", "nothing")) else "changed the outline"
                if "still moving" in did:
                    kind += " (still moving after the click)"
                kinds[kind] = kinds.get(kind, 0) + n
            lines += [f"    {n:4}x click: {k[:240]}" for k, n in sorted(kinds.items(), key=lambda kv: -kv[1])]
            counts = {}
            for l in r.get("said", []):
                counts[l] = counts.get(l, 0) + 1
            lines += [f"    {n:4}x {l[:240]}" for l, n in sorted(counts.items(), key=lambda kv: -kv[1])]
            print("\n".join(lines), file=s, flush=True)
            print(head, flush=True)


if __name__ == "__main__":
    main()
