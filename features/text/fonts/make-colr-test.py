#!/usr/bin/env python3
"""Writes LjosColrTest-Regular.ttf: a colour outline font of two characters.

Colour emoji fonts on Windows (Segoe UI Emoji) and on newer Android (Noto
Color Emoji) draw their glyphs as layers of outlines, each filled with a
colour of the font's palette (COLR and CPAL tables), not as pictures. This
font is made the same way, small enough to read. At 20 pixels per em
(50 units a pixel), from the glyph origin, y up:

- U+F8F2 is a COLR version 0 glyph of three layers: red from 6 to 16
  pixels above the baseline, blue from 4 below to 6 above, both the em
  wide, and over the blue a square in the text's own colour (palette
  index 0xFFFF), 8 to 12 pixels across and 1 to 5 pixels up.
- U+F8F3 is a COLR version 1 glyph of the same three layers, the blue one
  a gradient from blue at the left edge to green at the right.

Both base glyphs also have a plain outline, the whole box from 4 pixels
below the baseline to 16 above, as a font without colour support draws
them. The advance is the em, 1000 units of 1000.

Run it from this directory: `python3 make-colr-test.py`. Only the standard
library is used, so the file can be made again anywhere.
"""

import struct

UPM = 1000
V0_CHAR = 0xF8F2
V1_CHAR = 0xF8F3
NOTDEF_ADVANCE = 500
TOP = 800
BOTTOM = -200
SPLIT = 300
# Glyph ids: 0 .notdef, 1 the version 0 base, 2 the version 1 base, then
# the three layers.
BOX = (0, BOTTOM, UPM, TOP)
LAYERS = [(0, SPLIT, UPM, TOP), (0, BOTTOM, UPM, SPLIT), (400, 50, 600, 250)]
OUTLINES = [None, BOX, BOX] + LAYERS
RED, BLUE, GREEN = 0, 1, 2
# CPAL colour records are blue, green, red, alpha.
PALETTE = [(0, 0, 255, 255), (255, 0, 0, 255), (0, 255, 0, 255)]
FOREGROUND = 0xFFFF
ONE = 0x4000  # 1.0 in F2DOT14


def checksum(data: bytes) -> int:
    data += b"\0" * (-len(data) % 4)
    return sum(struct.unpack(f">{len(data) // 4}I", data)) & 0xFFFFFFFF


def rect_glyph(box) -> bytes:
    x0, y0, x1, y1 = box
    # One clockwise contour of four on-curve points, coordinates as 16-bit
    # deltas (flag 0x01 alone).
    points = [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
    data = struct.pack(">hhhhh", 1, x0, y0, x1, y1)
    data += struct.pack(">HH", 3, 0) + bytes([0x01] * 4)
    last = (0, 0)
    xs, ys = b"", b""
    for x, y in points:
        xs += struct.pack(">h", x - last[0])
        ys += struct.pack(">h", y - last[1])
        last = (x, y)
    data += xs + ys
    return data + b"\0" * (-len(data) % 4)


def glyf_loca():
    glyf = b""
    loca = []
    for outline in OUTLINES:
        loca.append(len(glyf))
        if outline is not None:
            glyf += rect_glyph(outline)
    loca.append(len(glyf))
    return glyf, b"".join(struct.pack(">I", at) for at in loca)


def cpal() -> bytes:
    records = b"".join(bytes(c) for c in PALETTE)
    n = len(PALETTE)
    return struct.pack(">HHHHIH", 0, n, 1, n, 14, 0) + records


def u24(value: int) -> bytes:
    return struct.pack(">I", value)[1:]


def paint_solid(index) -> bytes:
    return struct.pack(">BHH", 2, index, ONE)


def paint_glyph(glyph, paint: bytes) -> bytes:
    # Format 10: the paint it fills follows, 6 bytes on.
    return struct.pack(">B", 10) + u24(6) + struct.pack(">H", glyph) + paint


def paint_linear(start_index, end_index) -> bytes:
    # Format 4: from the left edge to the right, the third point straight up
    # from the first so the gradient is not skewed; the colour line follows.
    head = struct.pack(">B", 4) + u24(16) + struct.pack(">hhhhhh", 0, 0, UPM, 0, 0, UPM)
    line = struct.pack(">BH", 0, 2)
    line += struct.pack(">hHh", 0, start_index, ONE)
    line += struct.pack(">hHh", ONE, end_index, ONE)
    return head + line


def colr() -> bytes:
    header = 34
    # Version 0 records: one base glyph, its three layers.
    base_v0 = struct.pack(">HHH", 1, 0, 3)
    layers_v0 = b"".join(
        struct.pack(">HH", g, p) for g, p in [(3, RED), (4, BLUE), (5, FOREGROUND)]
    )
    # Version 1: a layer list of three paints, a base glyph list of one,
    # whose paint is the layers.
    paints = [
        paint_glyph(3, paint_solid(RED)),
        paint_glyph(4, paint_linear(BLUE, GREEN)),
        paint_glyph(5, paint_solid(FOREGROUND)),
    ]
    layer_list = struct.pack(">I", len(paints))
    at = 4 + 4 * len(paints)
    body = b""
    for paint in paints:
        layer_list += struct.pack(">I", at + len(body))
        body += paint
    layer_list += body
    colr_layers = struct.pack(">BBI", 1, len(paints), 0)
    base_list = struct.pack(">I", 1) + struct.pack(">HI", 2, 4 + 6) + colr_layers

    base_v0_at = header
    layers_v0_at = base_v0_at + len(base_v0)
    base_list_at = layers_v0_at + len(layers_v0)
    layer_list_at = base_list_at + len(base_list)
    out = struct.pack(
        ">HHIIHIIIII",
        1, 1, base_v0_at, layers_v0_at, 3, base_list_at, layer_list_at, 0, 0, 0,
    )
    return out + base_v0 + layers_v0 + base_list + layer_list


def name_table(family):
    records = {
        1: family,
        2: "Regular",
        3: family + " Regular",
        4: family + " Regular",
        5: "Version 1.0",
        6: family.replace(" ", "") + "-Regular",
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
    # Format 4: the two characters, then the closing segment.
    segs = [(V0_CHAR, V1_CHAR, (1 - V0_CHAR) & 0xFFFF), (0xFFFF, 0xFFFF, 1)]
    n = len(segs)
    search = 2 ** (n.bit_length() - 1)
    f4 = struct.pack(">HHH", 4, 0, 0)  # length filled below
    f4 += struct.pack(">HHHH", n * 2, search * 2, search.bit_length() - 1, n * 2 - search * 2)
    f4 += b"".join(struct.pack(">H", e) for _, e, _ in segs)
    f4 += struct.pack(">H", 0)
    f4 += b"".join(struct.pack(">H", s) for s, _, _ in segs)
    f4 += b"".join(struct.pack(">H", d) for _, _, d in segs)
    f4 += b"".join(struct.pack(">H", 0) for _ in segs)
    f4 = f4[:2] + struct.pack(">H", len(f4)) + f4[4:]
    return struct.pack(">HH", 0, 1) + struct.pack(">HHI", 3, 1, 12) + f4


def build(family) -> bytes:
    advances = [NOTDEF_ADVANCE] + [UPM] * (len(OUTLINES) - 1)
    num = len(advances)
    glyf, loca = glyf_loca()

    tables = {}
    tables[b"head"] = struct.pack(
        ">IIIIHHqqhhhhHHhhh",
        0x00010000, 0x00010000, 0, 0x5F0F3CF5, 0x000B, UPM, 0, 0,
        0, BOTTOM, UPM, TOP, 0, 8, 2, 1, 0,
    )
    tables[b"hhea"] = struct.pack(
        ">IhhhHhhhhhhhhhhhH",
        0x00010000, 800, -200, 0, max(advances), 0, 0, UPM,
        1, 0, 0, 0, 0, 0, 0, 0, num,
    )
    tables[b"maxp"] = struct.pack(
        ">IHHHHHHHHHHHHHH", 0x00010000, num, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0,
    )
    # A left side bearing is the outline's left edge: FreeType places an
    # outline by it.
    bearings = [0 if o is None else o[0] for o in OUTLINES]
    tables[b"hmtx"] = b"".join(struct.pack(">Hh", a, b) for a, b in zip(advances, bearings))
    tables[b"loca"] = loca
    tables[b"glyf"] = glyf
    tables[b"cmap"] = cmap_table()
    tables[b"name"] = name_table(family)
    tables[b"COLR"] = colr()
    tables[b"CPAL"] = cpal()
    tables[b"post"] = struct.pack(">IIhhIIIII", 0x00030000, 0, -100, 50, 0, 0, 0, 0, 0)
    tables[b"OS/2"] = struct.pack(
        ">HhHHHhhhhhhhhhhh10sIIII4sHHHhhhHHII",
        4, UPM, 400, 5, 0, 650, 700, 0, 140, 650, 700, 0, 480, 50, 250, 0,
        b"\0" * 10, 0, 1 << 28, 0, 0, b"LJOS", 0x40,
        V0_CHAR, V1_CHAR, 800, -200, 0, 800, 200, 1, 0,
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
    with open("LjosColrTest-Regular.ttf", "wb") as f:
        f.write(build("Ljos Colr Test"))
