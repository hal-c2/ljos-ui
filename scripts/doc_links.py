#!/usr/bin/env python3
"""Checks that the relative links of Markdown files name files that exist.

    doc_links.py [FILE.md ...]    the files given, or every tracked *.md

Files under vendor/ and third_party/ are left out: they are copies of other
projects' trees, whose links point into trees not copied. So is skills/, which
ljos_mcp_author's tests write from docs/site (LJOS_SKILLS=write) and hold to it.

A link is `[text](target)` outside code; a target with a scheme (`https:`, `mailto:`),
a bare `#anchor` or a `<...>` placeholder is not a file and is left alone. The anchor of
a file link is not checked: headings change wording more often than files move.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

LINK = re.compile(r"(?<!!)\[[^\]\n]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)|!\[[^\]\n]*\]\(([^)\s]+)\)")
FENCE = re.compile(r"^\s*(```|~~~)")
SKIPPED = ("vendor/", "third_party/", "skills/")
CODE_SPAN = re.compile(r"`[^`\n]*`")


def links(text: str):
    """(line number, target) for each link outside fenced code and code spans."""
    fenced = False
    for n, line in enumerate(text.splitlines(), 1):
        if FENCE.match(line):
            fenced = not fenced
            continue
        if fenced:
            continue
        for m in LINK.finditer(CODE_SPAN.sub("", line)):
            yield n, m.group(1) or m.group(2)


def broken(path: Path, root: Path) -> list[str]:
    out = []
    for n, target in links(path.read_text(encoding="utf-8", errors="replace")):
        if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target) or target.startswith(("#", "<")):
            continue
        file = target.split("#", 1)[0].split("?", 1)[0]
        if not file:
            continue
        base = root if file.startswith("/") else path.parent
        if not (base / file.lstrip("/")).exists():
            out.append(f"{path.relative_to(root)}:{n}: no such file: {target}")
    return out


def main(argv: list[str]) -> int:
    root = Path(__file__).resolve().parent.parent
    if argv:
        files = [Path(a).resolve() for a in argv if a.endswith(".md") and Path(a).exists()]
    else:
        listed = subprocess.run(
            ["git", "ls-files", "*.md"], cwd=root, check=True, capture_output=True, text=True
        ).stdout.split()
        files = [
            root / f
            for f in listed
            if not f.startswith(SKIPPED) and (root / f).exists()
        ]
    problems = [p for f in files for p in broken(f, root)]
    for p in problems:
        print(p)
    print(f"doc_links: {len(files)} files, {len(problems)} broken links", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
