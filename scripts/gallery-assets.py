#!/usr/bin/env python3
"""What the build of an example downloads before it compiles anything: pictures, meshes and fonts
that are in no repository.

gallery-assets.py            which examples have such assets, and whether they are here
gallery-assets.py <id>...    download and unpack those of each, and put them where its build does

corpus/assets.json says where they come from. `places` maps a directory of the example to one of
the archive; `beside` puts each file of `listed` (a source of the example's that names them, one a
line, each under `url`) in a directory of the example. The files are kept in LJOS_CORPUS_ASSETS,
default <target>/corpus-assets/<id>, and linked into corpus/qtdoc, as the example's own build would
copy them there. The links are excluded from the submodule's status."""
import json, os, re, shutil, subprocess, sys, urllib.parse, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target")
CACHE = os.environ.get("LJOS_CORPUS_ASSETS") or os.path.join(TARGET, "corpus-assets")
QTDOC = os.path.join(ROOT, "corpus", "qtdoc")


def fetch(url, to):
    """Whole or not at all: half a file is not one to use next time."""
    os.makedirs(os.path.dirname(to), exist_ok=True)
    with urllib.request.urlopen(url) as answer, open(to + ".part", "wb") as out:
        shutil.copyfileobj(answer, out)
    os.replace(to + ".part", to)


def link(directory, place, to):
    """`place`, of the example at `directory`, is `to`; git in the submodule does not see it."""
    path = os.path.join(directory, place)
    if os.path.islink(path):
        os.unlink(path)
    elif os.path.exists(path):
        sys.exit(f"{path} is there and is not a link: the example's own, not to be replaced")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    os.symlink(to, path)
    git_dir = subprocess.run(["git", "-C", QTDOC, "rev-parse", "--absolute-git-dir"], capture_output=True, text=True, check=True).stdout.strip()
    exclude = os.path.join(git_dir, "info", "exclude")
    os.makedirs(os.path.dirname(exclude), exist_ok=True)
    line = "/" + os.path.relpath(path, QTDOC)
    have = open(exclude).read().splitlines() if os.path.exists(exclude) else []
    if line not in have:
        with open(exclude, "a") as out:
            out.write(line + "\n")


def listed(directory, spec, kept):
    with open(os.path.join(directory, spec["listed"])) as source:
        names = re.findall(r'^\s*"([^":]+\.\w+)",?\s*$', source.read(), re.M)
    if not names:
        sys.exit(f"{spec['listed']} names no files")
    for index, name in enumerate(names):
        file = os.path.join(kept, name)
        if not os.path.exists(file):
            print(f"{index + 1} of {len(names)}, {name}", flush=True)
            fetch(urllib.parse.urljoin(spec["url"], urllib.parse.quote(name)), file)
        link(directory, os.path.join(spec["beside"], name), file)


def archive(directory, spec, kept):
    done = os.path.join(kept, ".unpacked")
    if not os.path.exists(done):
        zip_file = os.path.join(kept, os.path.basename(urllib.parse.urlparse(spec["url"]).path))
        if not os.path.exists(zip_file):
            print(f"downloading {spec['url']}", flush=True)
            fetch(spec["url"], zip_file)
        without = spec.get("without", [])
        subprocess.run(["unzip", "-o", "-q", zip_file, "-d", kept] + (["-x"] + without if without else []), check=True)
        # Unpacked, the archive is the same again.
        os.unlink(zip_file)
        open(done, "w").close()
    for place, within in spec.get("places", {}).items():
        link(directory, place, os.path.join(kept, within))


def main():
    assets = json.load(open(os.path.join(ROOT, "corpus", "assets.json")))
    examples = {e["id"]: e for e in json.load(open(os.path.join(ROOT, "corpus", "examples.json")))["examples"]}
    if len(sys.argv) == 1:
        for eid, spec in assets.items():
            here = os.path.isdir(os.path.join(CACHE, eid))
            print(f"{eid}: {'here' if here else 'not here'} ({spec['url']})")
        return
    for eid in sys.argv[1:]:
        if eid not in assets:
            sys.exit(f"no assets to download for {eid}")
        directory = os.path.join(QTDOC, examples[eid]["dir"])
        kept = os.path.join(CACHE, eid)
        os.makedirs(kept, exist_ok=True)
        (listed if "listed" in assets[eid] else archive)(directory, assets[eid], kept)
        print(f"{eid}: {kept}")


main()
