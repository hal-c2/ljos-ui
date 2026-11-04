#!/usr/bin/env python3
"""Writes LjosSymbolsTest-Regular.ttf: a font of two private-use symbols.

Fallback by coverage needs a font that has characters no other font has, so
a scenario can tell which font a character was set in. No installed font has
U+F8F0 or U+10FFF0; this one has both, each with an advance of its own
(600 and 700 units of 1000), and nothing else but .notdef.

Run it from this directory: `python3 make-symbols-test.py`. Only the standard
library is used, so the file can be made again anywhere.
"""

import struct

UPM = 1000
FAMILY = "Ljos Symbols Test"
# (code point, advance, outline): a box and a triangle.
GLYPHS = [
    (0xF8F0, 600, [(50, 0), (550, 0), (550, 700), (50, 700)]),
    (0x10FFF0, 700, [(50, 0), (650, 350), (50, 700)]),
]
NOTDEF_ADVANCE = 500


def checksum(data: bytes) -> int:
    data += b"\0" * (-len(data) % 4)
    return sum(struct.unpack(f">{len(data) // 4}I", data)) & 0xFFFFFFFF


def glyph(points):
    xs = [p[0] for p in points]
    ys = [p[1] for p in points]
    out = struct.pack(">hhhhh", 1, min(xs), min(ys), max(xs), max(ys))
    out += struct.pack(">H", len(points) - 1)  # end point of the one contour
    out += struct.pack(">H", 0)  # no instructions
    out += bytes([0x01] * len(points))  # every point on the curve, long x and y
    x = y = 0
    dx = b""
    dy = b""
    for px, py in points:
        dx += struct.pack(">h", px - x)
        dy += struct.pack(">h", py - y)
        x, y = px, py
    out += dx + dy
    out += b"\0" * (-len(out) % 4)
    return out


def name_table():
    records = {
        1: FAMILY,
        2: "Regular",
        3: FAMILY + " Regular",
        4: FAMILY + " Regular",
        5: "Version 1.0",
        6: "LjosSymbolsTest-Regular",
    }
    strings = b""
    entries = b""
    for name_id, text in records.items():
        data = text.encode("utf-16-be")
        entries += struct.pack(">HHHHHH", 3, 1, 0x409, name_id, len(data), len(strings))
        strings += data
    count = len(records)
    return struct.pack(">HHH", 0, count, 6 + 12 * count) + entries + strings


def cmap_table():
    # Format 4 for the BMP character, format 12 for every character.
    bmp = [(c, i + 1) for i, (c, _, _) in enumerate(GLYPHS) if c <= 0xFFFF]
    segs = [(c, c, gid - c) for c, gid in bmp] + [(0xFFFF, 0xFFFF, 1)]
    n = len(segs)
    search = 2 ** (n.bit_length() - 1)
    f4 = struct.pack(">HHH", 4, 0, 0)  # length filled below
    f4 += struct.pack(">HHHH", n * 2, search * 2, search.bit_length() - 1, n * 2 - search * 2)
    f4 += b"".join(struct.pack(">H", e) for _, e, _ in segs)
    f4 += struct.pack(">H", 0)
    f4 += b"".join(struct.pack(">H", s) for s, _, _ in segs)
    f4 += b"".join(struct.pack(">H", d & 0xFFFF) for _, _, d in segs)
    f4 += b"".join(struct.pack(">H", 0) for _ in segs)
    f4 = f4[:2] + struct.pack(">H", len(f4)) + f4[4:]
    groups = [(c, c, i + 1) for i, (c, _, _) in enumerate(GLYPHS)]
    f12 = struct.pack(">HHIII", 12, 0, 16 + 12 * len(groups), 0, len(groups))
    f12 += b"".join(struct.pack(">III", *g) for g in groups)
    header = struct.pack(">HH", 0, 2)
    off4 = 4 + 8 * 2
    off12 = off4 + len(f4)
    header += struct.pack(">HHI", 3, 1, off4) + struct.pack(">HHI", 3, 10, off12)
    return header + f4 + f12


def build() -> bytes:
    glyf = b""
    loca = [0]
    outlines = [None] + [g[2] for g in GLYPHS]
    for points in outlines:
        if points:
            glyf += glyph(points)
        loca.append(len(glyf))
    advances = [NOTDEF_ADVANCE] + [g[1] for g in GLYPHS]
    all_points = [p for g in GLYPHS for p in g[2]]
    x_min = min(p[0] for p in all_points)
    x_max = max(p[0] for p in all_points)
    y_max = max(p[1] for p in all_points)
    num = len(advances)

    tables = {}
    tables[b"head"] = struct.pack(
        ">IIIIHHqqhhhhHHhhh",
        0x00010000, 0x00010000, 0, 0x5F0F3CF5, 0x000B, UPM, 0, 0,
        x_min, 0, x_max, y_max, 0, 8, 2, 1, 0,
    )
    tables[b"hhea"] = struct.pack(
        ">IhhhHhhhhhhhhhhhH",
        0x00010000, 800, -200, 0, max(advances), 0, 0, x_max,
        1, 0, 0, 0, 0, 0, 0, 0, num,
    )
    tables[b"maxp"] = struct.pack(
        ">IHHHHHHHHHHHHHH", 0x00010000, num, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0,
    )
    tables[b"hmtx"] = b"".join(struct.pack(">Hh", a, 0) for a in advances)
    tables[b"loca"] = b"".join(struct.pack(">I", o) for o in loca)
    tables[b"glyf"] = glyf
    tables[b"cmap"] = cmap_table()
    tables[b"name"] = name_table()
    tables[b"post"] = struct.pack(">IIhhIIIII", 0x00030000, 0, -100, 50, 0, 0, 0, 0, 0)
    tables[b"OS/2"] = struct.pack(
        ">HhHHHhhhhhhhhhhh10sIIII4sHHHhhhHHII",
        4, 600, 400, 5, 0, 650, 700, 0, 140, 650, 700, 0, 480, 50, 250, 0,
        b"\0" * 10, 0, 1 << 28, 0, 0, b"LJOS", 0x40,
        0xF8F0, 0xFFFF, 800, -200, 0, 800, 200, 1, 0,
    )
    tables[b"OS/2"] += struct.pack(">hhHHH", 500, 700, 0, 0x20, 0)

    tags = sorted(tables)
    n = len(tags)
    search = 2 ** (n.bit_length() - 1)
    out = struct.pack(">IHHHH", 0x00010000, n, search * 16, search.bit_length() - 1, n * 16 - search * 16)
    offset = 12 + 16 * n
    directory = b""
    body = b""
    for tag in tags:
        data = tables[tag]
        directory += struct.pack(">4sIII", tag, checksum(data), offset + len(body), len(data))
        body += data + b"\0" * (-len(data) % 4)
    font = bytearray(out + directory + body)
    head_at = offset + sum(len(tables[t]) + (-len(tables[t]) % 4) for t in tags[: tags.index(b"head")])
    adjust = (0xB1B0AFBA - checksum(bytes(font))) & 0xFFFFFFFF
    font[head_at + 8 : head_at + 12] = struct.pack(">I", adjust)
    return bytes(font)


if __name__ == "__main__":
    with open("LjosSymbolsTest-Regular.ttf", "wb") as f:
        f.write(build())
