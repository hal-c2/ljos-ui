#!/usr/bin/env bash
# bench-manifest.sh OUT [ids…]: writes OUT/manifest.json, the apps `ljos bench`
# measures and `mise run bench:hot` records: bench/*.qml and the corpus
# examples (by default all qml6 can run), each with its imports and
# arguments. An example named that qml6 cannot run here is measured without
# it ("qt": false). Each example runs as `mise run gallery:run` runs it: with
# its stand-ins (corpus/standins) and where its downloaded assets are
# (`mise run gallery:assets <id>` first).
# Needs target/debug/qml-compare (cargo build -p qml_compare).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"
out="$1"
shift
ids=("$@")
mkdir -p "$out"
manifest="$out/manifest.json"
overlays="$out/overlay"
# The examples that rendered in the last gallery run are the ones that run
# under both; with no gallery run, every runnable one.
gallery="${GALLERY_DIR:-$target/gallery}/report.json"
# Where `mise run gallery:drive` left the controls it clicked, per example.
drive="${GALLERY_DIR:-$target/gallery}/drive"
if [[ ${#ids[@]} -eq 0 ]]; then which=(--runnable); else which=("${ids[@]}"); fi
rows="$("$target/debug/qml-compare" list --manifest "$root/corpus/examples.json" "${which[@]}")"
while IFS=$'\t' read -r id dir _; do
  [[ -n $id ]] || continue
  rm -rf "${overlays:?}/$id"
  mkdir -p "$overlays/$id"
  # Examples built with CMake name their modules there; a qmldir per module
  # lets plain qml6 find them, as for the gallery's reference pictures.
  "$root/corpus/harness/overlay.sh" "$dir" "$overlays/$id"
done <<< "$rows"

ROWS="$rows" python3 - "$root" "$overlays" "$gallery" "$drive" "$manifest" ${ids[@]+"${ids[@]}"} <<'PY'
import json, os, re, sys
root, overlays, gallery, drive, manifest, *wanted = sys.argv[1:]
assets = json.load(open(os.path.join(root, "corpus", "assets.json")))
renders = None
try:
    renders = {e["id"] for e in json.load(open(gallery)) if e.get("renders")}
except OSError:
    pass
apps = []
for name in sorted(os.listdir(os.path.join(root, "bench"))):
    if not name.endswith(".qml"):
        continue
    path = os.path.join(root, "bench", name)
    text = open(path).read()
    w = re.search(r"\bwidth:\s*(\d+)", text)
    h = re.search(r"\bheight:\s*(\d+)", text)
    app = {"id": name[:-4], "file": path, "imports": [os.path.dirname(path)]}
    if w and h:
        app["size"] = f"{w.group(1)}x{h.group(1)}"
    apps.append(app)
for line in os.environ["ROWS"].splitlines():
    cid, d, entry, width, height, _settle, qt, *_ = line.split("\t")
    # Named examples are measured even where the last gallery run had no picture.
    if not wanted and renders is not None and cid not in renders:
        continue
    apps.append({
        "id": cid,
        "file": os.path.join(d, entry),
        "imports": [d, os.path.join(overlays, cid)],
        # No size: Ljós takes the root's own, as qml6 does. The gallery's
        # capture size is not the root's for every example, and a window
        # of another size is another amount of work to compare.
        # The example's directory is its project, as for the gallery.
        "ljos_args": ["--project", d, "--example", cid],
    })
    if qt != "true":
        apps[-1]["qt"] = False
    # As `mise run gallery:run` starts it: corpus/standins/<id> holds QML for the C++ of the
    # example's own build, each module answering the import of its name (qml6 finds them by
    # -I), and that QML may read the example's files.
    standins = os.path.join(root, "corpus", "standins", cid)
    if os.path.isdir(standins):
        apps[-1]["imports"].append(standins)
        for here, dirs, files in sorted(os.walk(standins)):
            dirs[:] = sorted(x for x in dirs if x != "downloaded")
            if "qmldir" in files:
                module = re.search(r"^module\s+(\S+)", open(os.path.join(here, "qmldir")).read(), re.M)
                if module:
                    apps[-1]["ljos_args"] += ["--alias", f"{module.group(1)}={here}"]
        apps[-1]["env"] = {"QML_XHR_ALLOW_FILE_READ": "1"}
    # An example whose build downloads assets reads them by `file:content/…`, relative to where
    # it runs: the example's directory, or for a stand-in's download one whose `content` is it.
    if cid in assets:
        downloaded = os.path.join(standins, "downloaded")
        if os.path.isdir(downloaded):
            here = os.path.join(os.path.dirname(manifest), "cwd", cid)
            os.makedirs(here, exist_ok=True)
            content = os.path.join(downloaded, "content")
            link = os.path.join(here, "content")
            if os.path.islink(link):
                os.unlink(link)
            os.symlink(content if os.path.isdir(content) else downloaded, link)
            apps[-1]["dir"] = here
        else:
            apps[-1]["dir"] = d
    # The controls gallery:drive clicked, in its order, are clicked on every runtime; with
    # fewer than three (a game, a snippet of MouseAreas) gallery:drive used a grid, as the
    # bench does.
    try:
        points = json.load(open(os.path.join(drive, cid, "clicks.json"))).get("points") or []
    except (OSError, ValueError):
        points = []
    if len(points) >= 3:
        apps[-1]["clicks"] = points
if wanted:
    known = {a["id"] for a in apps}
    missing = [w for w in wanted if w not in known]
    if missing:
        sys.exit(f"no bench app {', '.join(missing)}; the apps are: {', '.join(sorted(known))}")
    apps = [a for a in apps if a["id"] in wanted]
json.dump({"apps": apps}, open(manifest, "w"), indent=1)
print(f"{len(apps)} apps in {manifest}", file=sys.stderr)
PY

