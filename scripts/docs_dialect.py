#!/usr/bin/env python3
"""Lint the site's code blocks for Ljós's dialect (docs/DIALECT.md).

An example outside the pages about Qt is written as Ljós writes it: no
`import Qt…` unless tests/docs_site.txt keeps it in Qt's QML for a gap,
no `qsTr` and its kin (Ljós translates with t`…`), and the styled
`Ljos.Controls` rather than `Ljos.Controls.Basic` unless the control is
not drawn by the style yet. tests/docs_site.rs checks that each example
compiles; this checks that it is written in the dialect the site teaches.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Pages about Qt, whose examples show Qt's forms beside Ljós's on purpose.
COMPAT_PAGES = {"language/porting-from-qt.md"}

# `<page>#<n>` of the examples that keep `Ljos.Controls.Basic` because
# Ljos.Controls does not have the control yet, each with the control.
# Every control Basic has is in Ljos.Controls today (DIALECT.md 4.16).
BASIC_GAPS: dict[str, str] = {}

# The code blocks that are an example's source: QML and its scripts.
CODE = {"qml", "js", "javascript", "mjs", "ts", "typescript"}

QT_IMPORT = re.compile(r"^\s*import\s+(Qt[\w.]*)", re.M)
BASIC_IMPORT = re.compile(r"^\s*import\s+Ljos\.Controls\.Basic\b", re.M)
QT_TR = re.compile(
    r"\b(qsTr|qsTranslate|qsTrId|qsTrNoOp|qsTranslateNoOp|qsTrIdNoOp"
    r"|QT_TR_NOOP|QT_TRANSLATE_NOOP|QT_TRID_NOOP)\b"
)


class Block:
    def __init__(self, page: str, line: int, tag: str, words: list[str], body: str, nth: int):
        self.page, self.line, self.tag, self.body, self.nth = page, line, tag, body, nth
        self.flags = {w for w in words if "=" not in w}
        self.attrs = dict(w.split("=", 1) for w in words if "=" in w)


def fence(line: str) -> tuple[int, str] | None:
    rest = line.lstrip()
    ticks = len(rest) - len(rest.lstrip("`"))
    return (ticks, rest[ticks:].strip()) if ticks >= 3 else None


def blocks(page: str, text: str) -> list[Block]:
    """The fenced blocks of a page, the `qml` ones numbered from 1 as
    tests/docs_site.rs numbers them."""
    lines = text.splitlines()
    out, nth, i = [], 0, 0
    while i < len(lines):
        opened = fence(lines[i])
        if opened is None:
            i += 1
            continue
        ticks, info = opened
        end = next(
            (j for j in range(i + 1, len(lines))
             if (f := fence(lines[j])) and f[0] >= ticks and not f[1]),
            len(lines),
        )
        words = info.split()
        tag = words[0] if words else ""
        if tag == "qml":
            nth += 1
        out.append(Block(page, i + 1, tag, words[1:], "\n".join(lines[i + 1:end]), nth))
        i = end + 1
    return out


def pending(path: Path) -> dict[str, list[str]]:
    """`<page>#<n>` to its reasons, from tests/docs_site.txt."""
    out = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        words = line.split("#", 1)[0].split()
        if len(words) >= 2:
            out[f"{words[0]}#{words[1]}"] = words[2:]
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("site", nargs="?", default=str(ROOT / "docs/site"),
                        help="the site's directory (default: docs/site)")
    parser.add_argument("--pending", default=str(ROOT / "crates/ljos_cli/tests/docs_site.txt"),
                        help="the examples kept in Qt's QML, and why (default: tests/docs_site.txt)")
    args = parser.parse_args()
    site, pending_path = Path(args.site), Path(args.pending)
    shown = args.site.rstrip("/")
    listed = pending(pending_path)
    gaps = {key for key, reasons in listed.items() if any(r.startswith("gap:") for r in reasons)}

    problems: list[str] = []
    examples, pages = 0, 0
    seen_basic: set[str] = set()
    for path in sorted(site.rglob("*.md")):
        page = path.relative_to(site).as_posix()
        if page in COMPAT_PAGES:
            continue
        text = path.read_text(encoding="utf-8")
        # tests/docs_site.rs reads backtick fences only, so a `~~~` block
        # would be an example nothing checks or numbers.
        for n, line in enumerate(text.splitlines(), 1):
            if line.lstrip().startswith("~~~"):
                problems.append(f"{shown}/{page}:{n}: a `~~~` fence; the site fences its blocks with backticks")
        found = [b for b in blocks(page, text) if b.tag in CODE]
        if not found:
            continue
        pages += 1
        # The groups whose root is kept in Qt's QML: their scripts are Qt's too.
        qt_groups = set()
        for block in found:
            key = f"{page}#{block.nth}"
            if block.tag == "qml" and key in gaps and QT_IMPORT.search(block.body):
                if "group" in block.attrs:
                    qt_groups.add(block.attrs["group"])
        for block in found:
            examples += 1
            key = f"{page}#{block.nth}"
            at = f"{shown}/{page}:{block.line}"
            if block.tag == "qml":
                at += f" ({key})"
            qt = QT_IMPORT.search(block.body)
            basic = BASIC_IMPORT.search(block.body)
            if basic:
                if block.tag == "qml" and key in BASIC_GAPS:
                    seen_basic.add(key)
                else:
                    problems.append(
                        f"{at}: `import Ljos.Controls.Basic` draws the Basic style; "
                        "import the styled Ljos.Controls"
                    )
            # A gap keeps Qt's imports and Qt's translation, not the Basic style.
            if block.tag == "qml" and key in gaps:
                if not qt:
                    problems.append(
                        f"{pending_path.name}: {page} {block.nth} is listed as kept in Qt's QML, "
                        "but it imports no Qt module; take it off the list"
                    )
                continue
            if block.attrs.get("group") in qt_groups:
                continue
            if qt:
                problems.append(
                    f"{at}: `import {qt.group(1)}` is Qt's; import Ljos.Core, Ljos.Ui, "
                    "Ljos.Layout or Ljos.Models, or list the example in the pending "
                    "examples with the gap that keeps it"
                )
            if tr := QT_TR.search(block.body):
                problems.append(f"{at}: `{tr.group(1)}` is Qt's; translate with t`…` from Ljos.Core")
    for key in sorted(set(BASIC_GAPS) - seen_basic):
        problems.append(
            f"scripts/docs_dialect.py: {key} keeps Ljos.Controls.Basic for "
            f"{BASIC_GAPS[key]}, but it no longer imports it; take it off BASIC_GAPS"
        )

    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        print(f"docs:dialect: {len(problems)} problems", file=sys.stderr)
        return 1
    print(f"docs:dialect: {examples} examples on {pages} page{'s' if pages != 1 else ''} "
          "in Ljós's dialect")
    return 0


if __name__ == "__main__":
    sys.exit(main())
