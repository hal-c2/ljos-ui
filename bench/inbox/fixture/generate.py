#!/usr/bin/env python3
"""Writes the inbox bench's fixture: 10 000 issues and 24 avatars.

Seeded and stdlib-only, so every run writes the same bytes on every
machine; `--check` says whether the files on disk are those bytes. The
same fixture feeds every implementation of the inbox app (Ljós and the
other toolkits it is compared with), so none of them is measured on
different data.
"""
import argparse
import hashlib
import json
import random
import struct
import sys
import zlib
from datetime import datetime, timedelta, timezone
from pathlib import Path

SEED = 20261127
COUNT = 10_000
# The fixture's "now": dates are relative to it, not to the clock, so a
# chart of the last twelve weeks shows the same bars on every day.
NOW = datetime(2026, 11, 27, 12, 0, tzinfo=timezone.utc)
ME = "Ada Okafor"

PEOPLE = [
    "Ada Okafor", "Bjarni Sigurðsson", "Chen Wei", "Dana Kowalski", "Elif Yılmaz",
    "Farah Haddad", "Gustavo Pereira", "Hana Suzuki", "Ingrid Larsen", "Jamal Carter",
    "Kavya Nair", "Lucía Fernández", "Mateo Rossi", "Nia Mensah", "Oskar Lindqvist",
    "Priya Raman", "Quentin Dubois", "Rosa Martínez", "Sven Halvorsen", "Tomás Novák",
    "Uma Patel", "Viktor Petrov", "Wen Li", "Yusuf Demir", "Zainab Bello",
    "Aino Virtanen", "Bruno Costa", "Camille Laurent", "Dmitri Volkov", "Esther Cohen",
    "李娜", "田中 優", "Олег Смирнов", "Σοφία Παππά",
]
TAGS = [
    "render", "crash", "layout", "text", "input", "a11y", "i18n", "perf", "memory",
    "docs", "build", "network", "audio", "video", "security", "ux", "regression", "flaky",
]
SUBJECTS = [
    "Renderer", "Text layout", "Scroll view", "Window resize", "Font fallback",
    "Image cache", "Keyboard focus", "Screen reader", "Date picker", "Search field",
    "Settings dialog", "Startup", "Theme switch", "Clipboard", "Drag and drop",
    "Tooltip", "Context menu", "File chooser", "Video player", "Audio output",
    "Network retry", "Login form", "Chart", "Table header", "Status bar",
]
PROBLEMS = [
    "crashes", "flickers", "leaks memory", "is slow", "renders blank",
    "ignores the system theme", "loses its state", "wraps mid-word", "freezes",
    "draws at the wrong scale", "drops input", "stutters", "shows stale data",
    "reads the wrong label", "fails to build", "times out",
]
CONTEXTS = [
    "after resume", "on a 4K display", "when the window is resized", "with right-to-left text",
    "in dark mode", "on first launch", "with 10 000 rows", "after a hot reload",
    "on Wayland", "with a screen reader", "while scrolling", "behind a proxy",
    "with reduced motion on", "in the Japanese locale", "on battery", "after an update",
]
SENTENCES = [
    "Steps to reproduce: open the {s} and {a}.",
    "It started after the last update; the previous build was fine.",
    "I can reproduce it every time on my laptop, but not on the desktop.",
    "The log shows nothing unusual until the moment it {p}.",
    "Expected: the {s} keeps working. Actual: it {p}.",
    "This blocks the release checklist for the next milestone.",
    "A workaround is to restart the application, which is not great.",
    "Attached is a screen recording; the problem shows at about four seconds.",
    "Profiling points at the layout pass, which runs far more often than it should.",
    "Memory grows by a few megabytes every minute while it is open.",
    "Could this be related to the font fallback change from last month?",
    "Users on the support forum report the same thing {c}.",
    "The accessibility tree still lists the old items after they are gone.",
    "Turning off animations does not change anything.",
    "It only happens when the list holds more than a few thousand items.",
    "I bisected it to a change in the input handling, but I am not sure yet.",
    "Happy to test a fix if someone can share a build.",
    "Severity is moderate: annoying, but there is no data loss.",
    "The same code path is used by the export dialog, which also {p}.",
    "We should add a regression test once the cause is known.",
]
ACTIONS = [
    "scroll quickly to the end", "switch the theme twice", "type into the search field",
    "resize the window from the corner", "open and close the dialog ten times",
    "change the language", "paste a long paragraph", "leave it idle overnight",
]
AVATARS = 24


def body(rng, subject, problem, context):
    paragraphs = []
    for _ in range(rng.randint(1, 3)):
        n = rng.randint(2, 4)
        words = {"s": subject.lower(), "p": problem, "c": context, "a": rng.choice(ACTIONS)}
        paragraphs.append(" ".join(rng.choice(SENTENCES).format(**words) for _ in range(n)))
    return "\n\n".join(paragraphs)


def iso(t):
    return t.strftime("%Y-%m-%dT%H:%M:%SZ")


def issues():
    rng = random.Random(SEED)
    rows = []
    for i in range(COUNT):
        subject, problem, context = rng.choice(SUBJECTS), rng.choice(PROBLEMS), rng.choice(CONTEXTS)
        # Most issues are recent, as in a busy tracker: the age is skewed
        # towards today, up to a year back.
        age = timedelta(minutes=int((rng.random() ** 2) * 365 * 24 * 60))
        created = NOW - age
        updated = created + timedelta(minutes=int(rng.random() * age.total_seconds() / 60))
        author = rng.randrange(len(PEOPLE))
        tags = sorted(rng.sample(TAGS, rng.choice([0, 1, 1, 2, 2, 2, 3])))
        if problem == "crashes" and "crash" not in tags:
            tags = sorted(tags + ["crash"])
        rows.append({
            "id": i + 1,
            "key": f"INB-{i + 1}",
            "title": f"{subject} {problem} {context}",
            "author": PEOPLE[author],
            "avatar": f"avatars/{author % AVATARS:02d}.png",
            "assignee": rng.choice(PEOPLE + ["", "", ""]),
            "status": "closed" if rng.random() < 0.3 else "open",
            "priority": rng.choice([0, 1, 1, 2, 2, 2, 2, 3, 3]),
            "tags": tags,
            "starred": rng.random() < 0.08,
            "comments": int(rng.random() ** 3 * 40),
            "created": iso(created),
            "updated": iso(updated),
            "body": body(rng, subject, problem, context),
        })
    return rows


def png(width, height, pixels):
    """An RGB PNG from rows of (r, g, b) tuples."""
    raw = b"".join(b"\x00" + bytes(c for px in row for c in px) for row in pixels)

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def avatar(n):
    """A 96 px identicon: a mirrored 5 × 5 pattern in one of the person's colours."""
    rng = random.Random(SEED * 31 + n)
    hue = n / AVATARS
    import colorsys
    fg = tuple(int(c * 255) for c in colorsys.hls_to_rgb(hue, 0.45, 0.65))
    bg = tuple(int(c * 255) for c in colorsys.hls_to_rgb(hue, 0.92, 0.5))
    cells = [[rng.random() < 0.5 for _ in range(3)] for _ in range(5)]
    size, cell, pad = 96, 16, 8
    rows = []
    for y in range(size):
        row = []
        for x in range(size):
            cx, cy = (x - pad) // cell, (y - pad) // cell
            on = 0 <= cx < 5 and 0 <= cy < 5 and cells[cy][min(cx, 4 - cx)]
            row.append(fg if on else bg)
        rows.append(row)
    return png(size, size, rows)


def files():
    text = "[\n" + ",\n".join(json.dumps(r, ensure_ascii=False, separators=(",", ":")) for r in issues()) + "\n]\n"
    out = {"issues.json": text.encode()}
    for n in range(AVATARS):
        out[f"avatars/{n:02d}.png"] = avatar(n)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("dir", nargs="?", default=str(Path(__file__).resolve().parent.parent / "data"))
    ap.add_argument("--check", action="store_true", help="only say whether the files on disk are the fixture")
    args = ap.parse_args()
    root = Path(args.dir)
    stale = []
    for name, data in files().items():
        path = root / name
        if args.check:
            if not path.is_file() or path.read_bytes() != data:
                stale.append(name)
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.is_file() or path.read_bytes() != data:
            path.write_bytes(data)
    if args.check and stale:
        print(f"{root}: not the fixture: {', '.join(stale)}", file=sys.stderr)
        sys.exit(1)
    digest = hashlib.sha256((root / "issues.json").read_bytes()).hexdigest()
    print(f"{root}/issues.json sha256 {digest}")


if __name__ == "__main__":
    main()
