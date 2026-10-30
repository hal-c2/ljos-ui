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

FLOAT, UBYTE, USHORT = 5126, 5121, 5123
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

    def ubytes(self, rows, kind):
        raw = b"".join(bytes(r) for r in rows)
        self.accessors.append(
            {
                "bufferView": self.view(raw, ARRAY_BUFFER),
                "componentType": UBYTE,
                "count": len(rows),
                "type": kind,
            }
        )
        return len(self.accessors) - 1

    def matrices(self, items):
        """MAT4 accessors are not vertex data, so their view has no target."""
        raw = b"".join(struct.pack("<16f", *m) for m in items)
        self.accessors.append(
            {"bufferView": self.view(raw), "componentType": FLOAT, "count": len(items), "type": "MAT4"}
        )
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


def checker_gltf():
    """An unlit square 100 units across whose texture is four colours, the
    image's top row red and green and its bottom row blue and white, read
    nearest so each quadrant is one colour. The UVs put the image's top at
    the square's top, as glTF lays them out."""
    buf = Buffer()
    attributes, indices = square(buf, 50)
    image = png(2, 2, [[(255, 0, 0, 255), (0, 255, 0, 255)], [(0, 0, 255, 255), (255, 255, 255, 255)]])
    data = "data:application/octet-stream;base64," + base64.b64encode(buf.data).decode()
    picture = "data:image/png;base64," + base64.b64encode(image).decode()
    material = unlit("checker", (1, 1, 1))
    material["pbrMetallicRoughness"]["baseColorTexture"] = {"index": 0}
    document = {
        "asset": {"version": "2.0", "generator": "features/quick3d/models/gen.py"},
        "extensionsUsed": ["KHR_materials_unlit"],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "checker", "mesh": 0}],
        "meshes": [{"primitives": [{"attributes": attributes, "indices": indices, "material": 0}]}],
        "materials": [material],
        "textures": [{"source": 0, "sampler": 0}],
        "samplers": [{"magFilter": 9728, "minFilter": 9728, "wrapS": 33071, "wrapT": 33071}],
        "images": [{"uri": picture}],
        "buffers": [{"uri": data, "byteLength": len(buf.data)}],
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    }
    write(HERE / "checker.gltf", json.dumps(document, indent=1).encode())


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


def embedded(buf):
    return [{"uri": "data:application/octet-stream;base64," + base64.b64encode(buf.data).decode(), "byteLength": len(buf.data)}]


def quadrants_png():
    """Four quadrants of one colour each, as the uv-sets scenarios read
    them: red top left, green top right, blue bottom left, white bottom
    right."""
    red, green, blue, white = (255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 255, 255), (255, 255, 255, 255)
    return png(2, 2, [[red, green], [blue, white]])


def uv2_gltf():
    """An unlit square 100 units across whose texture is read through its
    second UV set. Every corner's first UV is in the red quadrant, so a
    reader that ignores TEXCOORD_1 draws it all red."""
    buf = Buffer()
    attributes, indices = square(buf, 50)
    attributes["TEXCOORD_1"] = attributes["TEXCOORD_0"]
    attributes["TEXCOORD_0"] = buf.floats([(0.25, 0.25)] * 4, "VEC2")
    material = unlit("quadrants", (1, 1, 1))
    material["pbrMetallicRoughness"]["baseColorTexture"] = {"index": 0, "texCoord": 1}
    picture = "data:image/png;base64," + base64.b64encode(quadrants_png()).decode()
    document = {
        "asset": {"version": "2.0", "generator": "features/quick3d/models/gen.py"},
        "extensionsUsed": ["KHR_materials_unlit"],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "square", "mesh": 0}],
        "meshes": [{"primitives": [{"attributes": attributes, "indices": indices, "material": 0}]}],
        "materials": [material],
        "textures": [{"source": 0, "sampler": 0}],
        "samplers": [{"magFilter": 9728, "minFilter": 9728, "wrapS": 33071, "wrapT": 33071}],
        "images": [{"uri": picture}],
        "buffers": embedded(buf),
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    }
    write(HERE / "uv2.gltf", json.dumps(document, indent=1).encode())


def skin_gltf():
    """A strip 20 units wide and 100 tall, from the origin up, skinned to
    two joints as an exporter writes it: joint indices as unsigned bytes,
    weights as floats, inverse bind matrices in their own view. Its lower
    two rows follow "hip" at the origin, its top row "knee", 50 units up
    and turned a quarter turn about Z, which swings the top over to the
    left. The mesh's node is moved 500 units right: a skinned mesh's own
    node is ignored."""
    buf = Buffer()
    rows = (0, 50, 100)
    positions = [(x, y, 0) for y in rows for x in (-10, 10)]
    attributes = {
        "POSITION": buf.floats(positions, "VEC3", bounds=True),
        "NORMAL": buf.floats([(0, 0, 1)] * len(positions), "VEC3"),
        "JOINTS_0": buf.ubytes([(0 if y < 100 else 1, 0, 0, 0) for (_, y, _) in positions], "VEC4"),
        "WEIGHTS_0": buf.floats([(1, 0, 0, 0)] * len(positions), "VEC4"),
    }
    indices = buf.indices([0, 1, 3, 0, 3, 2, 2, 3, 5, 2, 5, 4])
    # Column-major: the translation is the last four numbers.
    identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
    down = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, -50, 0, 1]
    half = math.sqrt(0.5)
    document = {
        "asset": {"version": "2.0", "generator": "features/quick3d/models/gen.py"},
        "extensionsUsed": ["KHR_materials_unlit"],
        "scene": 0,
        "scenes": [{"nodes": [0, 2]}],
        "nodes": [
            {"name": "hip", "children": [1]},
            {"name": "knee", "translation": [0, 50, 0], "rotation": [0, 0, half, half]},
            {"name": "strip", "mesh": 0, "skin": 0, "translation": [500, 0, 0]},
        ],
        "skins": [{"joints": [0, 1], "inverseBindMatrices": buf.matrices([identity, down]), "skeleton": 0}],
        "meshes": [{"primitives": [{"attributes": attributes, "indices": indices, "material": 0}]}],
        "materials": [unlit("red", (1, 0, 0))],
        "buffers": embedded(buf),
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    }
    write(HERE / "skin.gltf", json.dumps(document, indent=1).encode())


def morph_gltf():
    """An unlit red square 100 units across with one morph target that
    moves every vertex 100 units right, weighted 0.5 by the mesh."""
    buf = Buffer()
    attributes, indices = square(buf, 50)
    target = {"POSITION": buf.floats([(100, 0, 0)] * 4, "VEC3", bounds=True)}
    document = {
        "asset": {"version": "2.0", "generator": "features/quick3d/models/gen.py"},
        "extensionsUsed": ["KHR_materials_unlit"],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "square", "mesh": 0}],
        "meshes": [
            {
                "primitives": [{"attributes": attributes, "indices": indices, "material": 0, "targets": [target]}],
                "weights": [0.5],
            }
        ],
        "materials": [unlit("red", (1, 0, 0))],
        "buffers": embedded(buf),
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    }
    write(HERE / "morph.gltf", json.dumps(document, indent=1).encode())


if __name__ == "__main__":
    square_gltf()
    pair_gltf()
    checker_gltf()
    cube_glb()
    sphere_grid_glb()
    uv2_gltf()
    skin_gltf()
    morph_gltf()
