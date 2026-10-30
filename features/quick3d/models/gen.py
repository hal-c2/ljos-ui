#!/usr/bin/env python3
"""Writes the glTF models the RuntimeLoader scenarios and bench load.

Run from anywhere: python3 features/quick3d/models/gen.py

The files are written by hand rather than exported from a modelling tool so
that every number a scenario checks (sizes, positions, colours) is visible
here, and so the repository holds no asset it cannot rebuild. Only the
standard library is used.
"""

import base64
import json
import math
import struct
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
BENCH = HERE.parent.parent.parent / "bench" / "quick3d"

FLOAT, USHORT = 5126, 5123
ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER = 34962, 34963


class Buffer:
    """One glTF buffer, its views and accessors, built up in order."""

    def __init__(self):
        self.data = bytearray()
        self.views = []
        self.accessors = []

    def _align(self):
        while len(self.data) % 4:
            self.data.append(0)

    def view(self, raw, target=None):
        self._align()
        view = {"buffer": 0, "byteOffset": len(self.data), "byteLength": len(raw)}
        if target is not None:
            view["target"] = target
        self.data.extend(raw)
        self.views.append(view)
        return len(self.views) - 1

    def floats(self, rows, kind, bounds=False):
        width = len(rows[0])
        raw = b"".join(struct.pack("<%df" % width, *r) for r in rows)
        accessor = {
            "bufferView": self.view(raw, ARRAY_BUFFER),
            "componentType": FLOAT,
            "count": len(rows),
            "type": kind,
        }
        if bounds:
            accessor["min"] = [min(r[i] for r in rows) for i in range(width)]
            accessor["max"] = [max(r[i] for r in rows) for i in range(width)]
        self.accessors.append(accessor)
        return len(self.accessors) - 1

    def indices(self, items):
        raw = struct.pack("<%dH" % len(items), *items)
        self.accessors.append(
            {
                "bufferView": self.view(raw, ELEMENT_ARRAY_BUFFER),
                "componentType": USHORT,
                "count": len(items),
                "type": "SCALAR",
            }
        )
        return len(self.accessors) - 1


def square(buf, half):
    """A square facing +Z, `half` units from its centre to each side."""
    p = [(-half, -half, 0), (half, -half, 0), (half, half, 0), (-half, half, 0)]
    attributes = {
        "POSITION": buf.floats(p, "VEC3", bounds=True),
        "NORMAL": buf.floats([(0, 0, 1)] * 4, "VEC3"),
        "TEXCOORD_0": buf.floats([(0, 1), (1, 1), (1, 0), (0, 0)], "VEC2"),
    }
    return attributes, buf.indices([0, 1, 2, 0, 2, 3])


def unlit(name, rgb):
    # The factor is linear; 1 and 0 are the same in sRGB, so these colours
    # come out exactly as written.
    return {
        "name": name,
        "pbrMetallicRoughness": {"baseColorFactor": [*rgb, 1.0]},
        "extensions": {"KHR_materials_unlit": {}},
    }


def png(width, height, pixels):
    """An RGBA PNG; `pixels` is rows of (r, g, b, a)."""

    def chunk(kind, body):
        return (
            struct.pack(">I", len(body))
            + kind
            + body
            + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)
        )

    raw = b"".join(b"\0" + b"".join(bytes(p) for p in row) for row in pixels)
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def glb(document, binary):
    while len(binary) % 4:
        binary += b"\0"
    text = json.dumps(document, separators=(",", ":")).encode()
    while len(text) % 4:
        text += b" "
    body = (
        struct.pack("<II", len(text), 0x4E4F534A)
        + text
        + struct.pack("<II", len(binary), 0x004E4942)
        + binary
    )
    return struct.pack("<III", 0x46546C67, 2, 12 + len(body)) + body


def write(path, data):
    path.write_bytes(data)
    assert len(data) < 64 * 1024, f"{path.name} is {len(data)} bytes"
    print(f"{path.relative_to(HERE.parent.parent.parent)}: {len(data)} bytes")


def square_gltf():
    buf = Buffer()
    attributes, indices = square(buf, 50)
    uri = "data:application/octet-stream;base64," + base64.b64encode(buf.data).decode()
    document = {
        "asset": {"version": "2.0", "generator": "features/quick3d/models/gen.py"},
        "extensionsUsed": ["KHR_materials_unlit"],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "square", "mesh": 0}],
        "meshes": [{"primitives": [{"attributes": attributes, "indices": indices, "material": 0}]}],
        "materials": [unlit("red", (1, 0, 0))],
        "buffers": [{"uri": uri, "byteLength": len(buf.data)}],
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    }
    write(HERE / "square.gltf", json.dumps(document, indent=1).encode())


def pair_gltf():
    buf = Buffer()
    attributes, indices = square(buf, 20)
    document = {
        "asset": {"version": "2.0", "generator": "features/quick3d/models/gen.py"},
        "extensionsUsed": ["KHR_materials_unlit"],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [
            {"name": "pair", "children": [1, 2]},
            # Column-major: the translation is the last four numbers.
            {
                "name": "left",
                "mesh": 0,
                "matrix": [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, -50, 0, 0, 1],
            },
            {"name": "right", "mesh": 1, "translation": [50, 0, 0], "scale": [0.5, 0.5, 0.5]},
        ],
        "meshes": [
            {"primitives": [{"attributes": attributes, "indices": indices, "material": 0}]},
            {"primitives": [{"attributes": attributes, "indices": indices, "material": 1}]},
        ],
        "materials": [unlit("green", (0, 1, 0)), unlit("blue", (0, 0, 1))],
        "buffers": [{"uri": "pair.bin", "byteLength": len(buf.data)}],
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    }
    write(HERE / "pair.bin", bytes(buf.data))
    write(HERE / "pair.gltf", json.dumps(document, indent=1).encode())


def cube_glb():
    buf = Buffer()
    positions, normals, uvs, tangents, indices = [], [], [], [], []
    # Each face: its normal, and two axes along it that make a
    # counter-clockwise square seen from outside.
    faces = [
        ((1, 0, 0), (0, 0, -1), (0, 1, 0)),
        ((-1, 0, 0), (0, 0, 1), (0, 1, 0)),
        ((0, 1, 0), (1, 0, 0), (0, 0, -1)),
        ((0, -1, 0), (1, 0, 0), (0, 0, 1)),
        ((0, 0, 1), (1, 0, 0), (0, 1, 0)),
        ((0, 0, -1), (-1, 0, 0), (0, 1, 0)),
    ]
    for n, u, v in faces:
        base = len(positions)
        for su, sv, uv in ((-1, -1, (0, 1)), (1, -1, (1, 1)), (1, 1, (1, 0)), (-1, 1, (0, 0))):
            positions.append(tuple(50 * (n[i] + su * u[i] + sv * v[i]) for i in range(3)))
            normals.append(n)
            uvs.append(uv)
            tangents.append((*u, 1.0))
        indices += [base, base + 1, base + 2, base, base + 2, base + 3]
    attributes = {
        "POSITION": buf.floats(positions, "VEC3", bounds=True),
        "NORMAL": buf.floats(normals, "VEC3"),
        "TEXCOORD_0": buf.floats(uvs, "VEC2"),
        "TANGENT": buf.floats(tangents, "VEC4"),
    }
    index_accessor = buf.indices(indices)
    light, dark = (255, 255, 255, 255), (160, 160, 160, 255)
    checker = [[light if (x + y) % 2 == 0 else dark for x in range(4)] for y in range(4)]
    image_view = buf.view(png(4, 4, checker))
    document = {
        "asset": {"version": "2.0", "generator": "features/quick3d/models/gen.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "tall", "scale": [1, 2, 1], "children": [1]}, {"name": "cube", "mesh": 0}],
        "meshes": [
            {"primitives": [{"attributes": attributes, "indices": index_accessor, "material": 0}]}
        ],
        "materials": [
            {
                "name": "orange",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [1.0, 0.35, 0.05, 1.0],
                    "baseColorTexture": {"index": 0, "texCoord": 0},
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.5,
                },
            }
        ],
        "textures": [{"source": 0, "sampler": 0}],
        "samplers": [{"magFilter": 9728, "minFilter": 9728, "wrapS": 33071, "wrapT": 33071}],
        "images": [{"bufferView": image_view, "mimeType": "image/png"}],
        "buffers": [{"byteLength": len(buf.data)}],
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    }
    document["buffers"][0]["byteLength"] = (len(buf.data) + 3) // 4 * 4
    write(HERE / "cube.glb", glb(document, bytes(buf.data)))


def sphere_grid_glb():
    """One lit sphere drawn by nine nodes on a grid: about 5000 triangles."""
    buf = Buffer()
    segments, rings, radius = 24, 12, 40.0
    positions, normals, indices = [], [], []
    for r in range(rings + 1):
        theta = math.pi * r / rings
        for s in range(segments + 1):
            phi = 2 * math.pi * s / segments
            n = (math.sin(theta) * math.cos(phi), math.cos(theta), -math.sin(theta) * math.sin(phi))
            normals.append(n)
            positions.append(tuple(radius * c for c in n))
    for r in range(rings):
        for s in range(segments):
            a = r * (segments + 1) + s
            b = a + segments + 1
            indices += [a, b, a + 1, a + 1, b, b + 1]
    attributes = {
        "POSITION": buf.floats(positions, "VEC3", bounds=True),
        "NORMAL": buf.floats(normals, "VEC3"),
    }
    index_accessor = buf.indices(indices)
    grid = [
        {"name": f"sphere {x},{y}", "mesh": 0, "translation": [x * 100.0, y * 100.0, 0.0]}
        for y in (-1, 0, 1)
        for x in (-1, 0, 1)
    ]
    document = {
        "asset": {"version": "2.0", "generator": "features/quick3d/models/gen.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "grid", "children": list(range(1, len(grid) + 1))}, *grid],
        "meshes": [
            {"primitives": [{"attributes": attributes, "indices": index_accessor, "material": 0}]}
        ],
        "materials": [
            {
                "name": "steel",
                "pbrMetallicRoughness": {
                    "baseColorFactor": [0.6, 0.65, 0.7, 1.0],
                    "metallicFactor": 0.8,
                    "roughnessFactor": 0.3,
                },
            }
        ],
        "buffers": [{"byteLength": (len(buf.data) + 3) // 4 * 4}],
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    }
    BENCH.mkdir(exist_ok=True)
    write(BENCH / "sphere-grid.glb", glb(document, bytes(buf.data)))


if __name__ == "__main__":
    square_gltf()
    pair_gltf()
    cube_glb()
    sphere_grid_glb()
