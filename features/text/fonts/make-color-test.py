#!/usr/bin/env python3
"""Writes LjosColorTest-Regular.ttf: a colour bitmap font of one picture.

Colour emoji fonts on Linux and Android (Noto Color Emoji) keep their
glyphs as PNG pictures in the CBLC and CBDT tables, with no outlines. This
font is made the same way, small enough to read: U+F8F1 (private use) and
U+2614 (an emoji by its Emoji_Presentation) are the same picture of 20 by 20
pixels at 20 pixels per em, red above and blue below, with its top 16 pixels
above the baseline. Its advance is the em, 1000 units of 1000.

A picture's colours are its own, so a scenario can tell that it was drawn
as a picture, whatever colour the text is.

Run it from this directory: `python3 make-color-test.py`. Only the standard
library is used, so the file can be made again anywhere.
"""

import struct
import zlib

UPM = 1000
CHAR = 0xF8F1
EMOJI = 0x2614
PPEM = 20
SIDE = 20
ABOVE = 16
RED = (255, 0, 0, 255)
BLUE = (0, 0, 255, 255)
NOTDEF_ADVANCE = 500


def checksum(data: bytes) -> int:
    data += b"\0" * (-len(data) % 4)
    return sum(struct.unpack(f">{len(data) // 4}I", data)) & 0xFFFFFFFF


def png(width, height, pixel) -> bytes:
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    rows = b"".join(
        b"\0" + b"".join(bytes(pixel(x, y)) for x in range(width)) for y in range(height)
    )
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )


def picture() -> bytes:
    return png(SIDE, SIDE, lambda x, y: RED if y < SIDE // 2 else BLUE)


def cbdt_cblc():
    image = picture()
    # Format 17: small metrics, then the PNG.
    glyph = struct.pack(">BBbbB", SIDE, SIDE, 0, ABOVE, SIDE)
    glyph += struct.pack(">I", len(image)) + image
    cbdt = struct.pack(">HH", 3, 0) + glyph
    image_at = 4

    # One strike for glyph 1, one index subtable of format 1 (32-bit
    # offsets from the image data).
    subtable = struct.pack(">HHI", 1, 17, image_at) + struct.pack(">II", 0, len(glyph))
    array = struct.pack(">HHI", 1, 1, 8)
    index = array + subtable
    header = 8
    size_record = 48
    array_at = header + size_record
    line = struct.pack(">bbBbbbbbbbbb", ABOVE, ABOVE - SIDE, SIDE, 1, 0, 0, 0, 0, 0, 0, 0, 0)
    record = struct.pack(">IIII", array_at, len(index), 1, 0)
    record += line + line
    record += struct.pack(">HHBBBb", 1, 1, PPEM, PPEM, 32, 1)
    cblc = struct.pack(">HHI", 3, 0, 1) + record + index
    return cbdt, cblc


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
    # Format 4: both characters on glyph 1, in order, then the closing segment.
    segs = [(c, c, (1 - c) & 0xFFFF) for c in sorted((EMOJI, CHAR))]
    segs.append((0xFFFF, 0xFFFF, 1))
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
    advances = [NOTDEF_ADVANCE, UPM]
    num = len(advances)
    top = ABOVE * UPM // PPEM
    bottom = (ABOVE - SIDE) * UPM // PPEM
    cbdt, cblc = cbdt_cblc()

    tables = {}
    tables[b"head"] = struct.pack(
        ">IIIIHHqqhhhhHHhhh",
        0x00010000, 0x00010000, 0, 0x5F0F3CF5, 0x000B, UPM, 0, 0,
        0, bottom, UPM, top, 0, 8, 2, 1, 0,
    )
    tables[b"hhea"] = struct.pack(
        ">IhhhHhhhhhhhhhhhH",
        0x00010000, 800, -200, 0, max(advances), 0, 0, UPM,
        1, 0, 0, 0, 0, 0, 0, 0, num,
    )
    tables[b"maxp"] = struct.pack(
        ">IHHHHHHHHHHHHHH", 0x00010000, num, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0,
    )
    tables[b"hmtx"] = b"".join(struct.pack(">Hh", a, 0) for a in advances)
    # No outlines: every glyph is empty in `glyf`, as in a colour bitmap font.
    tables[b"loca"] = b"".join(struct.pack(">I", 0) for _ in range(num + 1))
    tables[b"glyf"] = b""
    tables[b"cmap"] = cmap_table()
    tables[b"name"] = name_table(family)
    tables[b"CBDT"] = cbdt
    tables[b"CBLC"] = cblc
    tables[b"post"] = struct.pack(">IIhhIIIII", 0x00030000, 0, -100, 50, 0, 0, 0, 0, 0)
    tables[b"OS/2"] = struct.pack(
        ">HhHHHhhhhhhhhhhh10sIIII4sHHHhhhHHII",
        4, UPM, 400, 5, 0, 650, 700, 0, 140, 650, 700, 0, 480, 50, 250, 0,
        # Ranges 46 (miscellaneous symbols) and 60 (private use).
        b"\0" * 10, 0, 1 << 14 | 1 << 28, 0, 0, b"LJOS", 0x40,
        EMOJI, CHAR, 800, -200, 0, 800, 200, 1, 0,
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
    with open("LjosColorTest-Regular.ttf", "wb") as f:
        f.write(build("Ljos Color Test"))
