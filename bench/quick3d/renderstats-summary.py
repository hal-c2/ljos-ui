#!/usr/bin/env python3
"""renderstats-summary.py LOGS OUT: sums up the lines renderstats-probe.qml
logged, one LOGS/<id>.log per scene, into OUT (JSON) and prints a table.

The first two seconds are left out: they are the window opening and the
scene's first frames. Each number is the median of the seconds after, for
the first View3D; frame and CPU times are Qt's, in milliseconds. A GPU time
of 0 is one Qt did not measure (an adapter without timestamps), so a scene
whose every second says 0 has none, not a GPU time of 0."""
import json
import os
import statistics
import sys

SKIP = 2
UNMEASURED_AT_ZERO = {"lastCompletedGpuTime"}
SHOWN = [
    ("fps", "fps"),
    ("frameTime", "frame ms"),
    ("syncTime", "sync ms"),
    ("renderPrepareTime", "prepare ms"),
    ("renderTime", "render ms"),
    ("lastCompletedGpuTime", "GPU ms"),
    ("drawCallCount", "draws"),
    ("renderPassCount", "passes"),
    ("pipelineCount", "pipelines"),
]


def summarise(path):
    lines, views, error = [], None, None
    for text in open(path, errors="replace"):
        text = text.strip()
        at = text.find("renderstats")
        if at < 0:
            continue
        tag, _, rest = text[at:].partition(" ")
        if tag == "renderstats-views":
            views = int(rest)
        elif tag == "renderstats-error":
            error = rest
        elif tag == "renderstats":
            try:
                lines.append(json.loads(rest))
            except ValueError:
                pass
    first = [l for l in lines if l.get("view") == 0 and l.get("second", 0) > SKIP]
    numbers = {}
    for name in sorted({k for l in first for k in l} - {"view", "second"}):
        values = [l[name] for l in first if isinstance(l.get(name), (int, float))]
        if name in UNMEASURED_AT_ZERO:
            values = [v for v in values if v > 0]
        if values:
            numbers[name] = statistics.median(values)
    return {"views": views, "seconds": len(first), "error": error, "median": numbers}


def main():
    logs, out = sys.argv[1:3]
    scenes = {}
    for name in sorted(os.listdir(logs)):
        if name.endswith(".log"):
            scenes[name[:-4]] = summarise(os.path.join(logs, name))
    json.dump(scenes, open(out, "w"), indent=1)
    print("| scene | " + " | ".join(h for _, h in SHOWN) + " |")
    print("|---|" + "---|" * len(SHOWN))
    for scene, s in scenes.items():
        m = s["median"]
        cells = [f"{m[k]:.2f}".rstrip("0").rstrip(".") if k in m else "–" for k, _ in SHOWN]
        note = f" ({s['error']})" if s["error"] else ""
        print(f"| {scene}{note} | " + " | ".join(cells) + " |")
    print(f"\nQt's renderStats, medians of each second after the first {SKIP}: {out}")


if __name__ == "__main__":
    main()
