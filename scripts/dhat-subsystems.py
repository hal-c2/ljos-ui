#!/usr/bin/env python3
"""dhat-subsystems.py <dhat-heap.json> [top]: the Rust heap a `dhat-heap`
build of ljos held when its profile was written, by crate (the first frame
outside the allocator and the standard library) and by call site."""
import json
import re
import sys
from collections import Counter

SKIP = re.compile(
    r"^(alloc::|core::|std::|<alloc::|<core::|<std::|dhat::|<dhat::|__rust|hashbrown::|"
    r"<hashbrown::|smallvec::|<smallvec::|indexmap::|<indexmap::|rust_begin|"
    r"\?\?\?|\[root\]|<T as |<\[T\]|<&|<\()"
)
CRATE = re.compile(r"^<?(?:<?[\w]+ as )?([a-z_][a-z0-9_]*)::")


def frame_name(f):
    # "0x1234: name (file:line)" or "0x1234: name"
    name = f.split(": ", 1)[1] if ": " in f else f
    return re.sub(r" \(.*\)$", "", name)


def main():
    path = sys.argv[1]
    top = int(sys.argv[2]) if len(sys.argv) > 2 else 20
    d = json.load(open(path))
    ftbl = d["ftbl"]
    by_crate = Counter()
    by_site = Counter()
    total = 0
    for pp in d["pps"]:
        eb = pp.get("eb", 0)
        if not eb:
            continue
        total += eb
        names = [frame_name(ftbl[i]) for i in pp["fs"]]
        own = [n for n in names if not SKIP.match(n)]
        crate = "?"
        if own:
            m = CRATE.match(own[0])
            crate = m.group(1) if m else own[0][:40]
        by_crate[crate] += eb
        by_site[" < ".join(n[:90] for n in own[:3]) or "?"] += eb
    print(f"{path}: {total / 2**20:.1f} MiB of Rust heap live")
    print("by crate:")
    for c, b in by_crate.most_common(top):
        print(f"  {b / 2**20:8.2f} MiB  {c}")
    print("by call site:")
    for s, b in by_site.most_common(top):
        print(f"  {b / 2**20:8.2f} MiB  {s}")


main()
