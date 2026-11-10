#!/usr/bin/env python3
"""Writes LjosSymbolsTest-Regular.woff2 and LjosSymbolsTest-Regular.woff.

They hold LjosSymbolsTest-Regular.ttf (made by make-symbols-test.py), with a
box drawn for .notdef as real fonts have, packed as the web packs fonts: WOFF2 with its glyphs transformed and Brotli-compressed, as
fontTools and Google's woff2 tools write it, and WOFF with each table
zlib-compressed. A scenario that measures its U+F8F0 can tell the decoded
font from any other.

Also writes LjosSymbolsTest-Truncated.woff2 and LjosSymbolsTest-Truncated.woff:
the first half of each, a header that promises tables the file does not have.

WOFF2 needs a Brotli encoder, which the standard library has not got, so run
it from this directory with fontTools:

    uv run --no-project --with fonttools==4.60.1 --with brotli==1.1.0 \
        python make-woff.py
"""

from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

SOURCE = "LjosSymbolsTest-Regular.ttf"

for flavor in ("woff2", "woff"):
    # The timestamps stay the TrueType file's, so the output is the same on
    # every run.
    font = TTFont(SOURCE, recalcTimestamp=False)
    # qml6 (6.12) refuses a WOFF2 whose transformed glyphs start with an
    # empty one, though the format allows it; with a box it loads the file,
    # so a scenario about the format runs on both.
    pen = TTGlyphPen(None)
    pen.moveTo((50, 0))
    pen.lineTo((450, 0))
    pen.lineTo((450, 700))
    pen.lineTo((50, 700))
    pen.closePath()
    font["glyf"][".notdef"] = pen.glyph()
    font.flavor = flavor
    out = f"LjosSymbolsTest-Regular.{flavor}"
    font.save(out, reorderTables=True)
    with open(out, "rb") as f:
        data = f.read()
    with open(f"LjosSymbolsTest-Truncated.{flavor}", "wb") as f:
        f.write(data[: len(data) // 2])
