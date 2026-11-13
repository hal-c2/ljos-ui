#!/usr/bin/env python3
"""Writes the textures bench/quick3d-stress.qml draws with.

Patterns, not photographs, so the files stay small in the repository while
the textures stay full size on the GPU: one of them is 2048 by 2048, which
is what a large texture costs to upload and to sample.

    python3 bench/quick3d/gen-stress.py
"""
import math
import os
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))


def png(name, size, pixel):
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            rows.extend(pixel(x, y))
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    header = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b""))
    with open(os.path.join(HERE, name), "wb") as f:
        f.write(data)


def checker(x, y):
    on = ((x // 32) + (y // 32)) % 2
    return (230, 230, 230) if on else (40, 60, 120)


def rings(x, y):
    band = int(math.hypot(x - 256, y - 256)) // 16 % 4
    return ((250, 200, 60), (220, 80, 40), (60, 160, 200), (30, 30, 60))[band]


def tiles(x, y):
    # Bricks of 128 by 64 with mortar, every other row shifted.
    row = y // 64
    bx = (x + (64 if row % 2 else 0)) % 128
    by = y % 64
    if bx < 4 or by < 4:
        return (200, 200, 190)
    shade = 150 + ((x // 128 + row * 7) % 5) * 15
    return (shade, 70, 50)


png("stress-checker.png", 256, checker)
png("stress-rings.png", 512, rings)
png("stress-tiles.png", 2048, tiles)
