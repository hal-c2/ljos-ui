#!/usr/bin/env python3
"""Says which of ci.yml's jobs a change needs, as `name=value` lines for $GITHUB_OUTPUT.

    full        "true" when everything runs
    clippy      "true" when fmt and clippy run; `clippy_args` names the packages
    wasm        "true" when the web build of ljos_mcp_core is checked
    tests       a JSON list of test shards, each {"name", "packages", "doc"}
    lint        "true" when features:lint runs
    features    a JSON list of feature shards, each {"name", "files"}
    acceptance  "true" when the acceptance suites run

`--mode full` runs everything (main, the nightly run, a dispatch). `--mode pr` runs what
the change between `--base` and `--head` can reach:

- A crate that changed, and every crate that depends on it (any kind of dependency, read
  from `cargo metadata`), is built, linted and tested.
- A file outside the crates that a crate's tests read (docs/design, corpus/, skills/, the
  media under features/multimedia, ...) tests that crate alone: READERS.
- A file a crate embeds from outside its directory (include_str!, include_bytes!) is part
  of that crate when its library or binaries embed it, and read by its tests otherwise.
- A .qml file anywhere but features/ also tests QML_READERS, whose tests read every one.
- A changed feature file runs; a changed fixture under features/<area>/ runs that area.
  A crate the features runner links runs every feature file, unless it is one of the
  crates in CONFINED, which serve QML modules that only some files import.
- Everything a path list here does not name runs everything. Each list names what is known
  to be safe to leave out, so a path no one thought of tests it all.

The weights the shards are balanced by are counted from the tree itself: test targets per
package, scenarios per feature file. The split is deterministic for a given tree.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath

# A change to any of these can change how every job runs, or what it runs on.
FULL = (
    "Cargo.lock",
    "Cargo.toml",
    "rust-toolchain",
    "rust-toolchain.toml",
    "mise.toml",
    ".gitmodules",
    ".cargo/",
    ".config/",
    ".github/",
    # The features runner, its steps and its filter: every scenario and the lint.
    "crates/ljos_bdd/",
    "scripts/ci_changes.py",
    "scripts/ci_changes_test.py",
    # The tasks CI runs.
    "mise-tasks/check/",
    "mise-tasks/test",
    "mise-tasks/features/",
    "mise-tasks/ci/",
)

# What none of the jobs here run: tools for the nightly gallery, the benchmarks and the
# desktop sandbox's own tasks, and files no build reads.
NOTHING = (
    ".gitignore",
    ".mcp.json",
    "LICENSE",
    "mise-tasks/",
    "scripts/bench-manifest.sh",
    "scripts/dhat-subsystems.py",
    "scripts/drag-in-probe.qml",
    "scripts/gallery-assets.py",
    "scripts/gallery-drive.py",
    "scripts/hot-drive.py",
    "scripts/own-home.sh",
)

# Files outside a crate that the crate's own tests read: a change to one tests those
# crates and nothing that depends on them, as their code did not change.
READERS = {
    "docs/design/": ("ljos_design", "ljos_mcp_author"),
    "docs/declarative.md": ("ljos_mcp_author",),
    # tests/docs_site.rs checks every example of the site.
    "docs/site/": ("ljos_cli",),
    "skills/": ("ljos_mcp_author",),
    "ledger/": ("ljos_bdd", "ljos_cli", "ljos_mcp_author"),
    "corpus/": (
        "ljos_3d",
        "ljos_cli",
        "ljos_compiler",
        "ljos_mcp_author",
        "ljos_shader",
        "qml_compare",
        "qml_fmt",
        "qml_syntax",
    ),
    "bench/": ("ljos_bench", "ljos_cli", "ljos_compiler", "qml_edit"),
    "features/README.md": ("ljos_mcp_author",),
    "features/multimedia/media/": ("ljos", "ljos_media"),
    "features/quick3d/models/": ("ljos_3d",),
    "scripts/cua-sandbox.sh": ("ljos_bdd",),
}

# Tests that read the QML files of the whole tree, wherever they are: the formatter's and
# the editor's round trips and the compiler's keys (qml_fmt, qml_edit and ljos_compiler's
# tests/corpus.rs and tests/keys.rs). A .qml file anywhere tests them too.
QML_READERS = ("ljos_compiler", "qml_edit", "qml_fmt")

# The feature files a path outside crates/ selects, besides its own directory's.
FEATURE_READERS = {
    "scripts/cua-sandbox.sh": ("tag", "@desktop"),
}

# Crates the features runner links whose behaviour only a QML module they serve shows:
# a change to one runs the feature files that import that module, or sit in the named
# directories, or carry the named tags. Any other crate the runner links runs every file.
CONFINED = {
    "ljos_blocks": {"imports": ("Ljos.Blocks",), "dirs": ("blocks",)},
    "ljos_folderlistmodel": {"imports": ("Qt.labs.folderlistmodel",)},
    "ljos_jsonmodel": {"imports": ("Ljos.Models",)},
    "ljos_localstorage": {"imports": ("QtQuick.LocalStorage",)},
    "ljos_lottie": {"imports": ("Qt.labs.lottieqt",), "dirs": ("lottie",)},
    "ljos_media": {"imports": ("QtMultimedia",), "dirs": ("multimedia",)},
    "ljos_physics": {"imports": ("QtQuick3D.Physics",)},
    "ljos_positioning": {"imports": ("QtPositioning",), "dirs": ("positioning",)},
    "ljos_quick3d": {"imports": ("QtQuick3D",), "dirs": ("quick3d",)},
    "ljos_quick3d_physics": {"imports": ("QtQuick3D.Physics",)},
    # Only the acceptance suites read a ratchet.
    "ljos_ratchet": {},
    "ljos_timeline": {"imports": ("QtQuick.Timeline",)},
    "ljos_webengine": {"imports": ("QtWebEngine",), "dirs": ("webengine",), "tags": ("@chromium",)},
    "ljos_xml": {"imports": ("QtQml.XmlListModel",)},
}

# `#[cfg(test)]`, or one that names `test` among others, as `#[cfg(all(test, feature = "x"))]`.
TEST_GATE = re.compile(r"#\[cfg\((?:test|(?:all|any)\([^\]]*\btest\b[^\]]*)\)\]")

# The crate the features runner is, and the one whose binary @desktop scenarios run.
RUNNER = "ljos_bdd"
DESKTOP_BIN = "ljos_cli"
# The crate whose web build `mise run check` checks.
WASM = "ljos_mcp_core"

# Shards: at most this many, each with at least about this much work.
TEST_SHARDS = 3
TEST_TARGETS_PER_SHARD = 20
FEATURE_SHARDS = 6
SCENARIOS_PER_SHARD = 1500


@dataclass
class Workspace:
    """The packages of the workspace, from `cargo metadata --no-deps`."""

    root: PurePosixPath
    # package name -> its directory, relative to the root
    dirs: dict[str, str]
    # package name -> workspace packages it depends on: (name, kind) pairs
    deps: dict[str, set[tuple[str, str]]]
    # paths outside the workspace members that a package depends on -> those packages
    path_deps: dict[str, set[str]]
    # package name -> how many test targets it has
    test_targets: dict[str, int]
    # package names with a library whose doctests run
    doctests: set[str]
    # a file a package's code embeds with include_str!/include_bytes! from outside its
    # own directory -> (package, "src"|"tests") pairs; see `scan_includes`
    includes: dict[str, set[tuple[str, str]]] = field(default_factory=dict)

    @classmethod
    def from_metadata(cls, meta: dict) -> Workspace:
        root = PurePosixPath(meta["workspace_root"])
        members = {p["name"]: p for p in meta["packages"]}
        dirs, deps, path_deps, test_targets, doctests = {}, {}, {}, {}, set()
        for name, p in members.items():
            dirs[name] = str(PurePosixPath(p["manifest_path"]).parent.relative_to(root))
            deps[name] = set()
            for d in p["dependencies"]:
                kind = d.get("kind") or "normal"
                if d["name"] in members:
                    deps[name].add((d["name"], kind))
                elif d.get("path"):
                    rel = str(PurePosixPath(d["path"]).relative_to(root))
                    path_deps.setdefault(rel, set()).add(name)
            test_targets[name] = sum(1 for t in p["targets"] if t.get("test", False))
            if any("lib" in t["kind"] and t.get("doctest", False) for t in p["targets"]):
                doctests.add(name)
        return cls(root, dirs, deps, path_deps, test_targets, doctests)

    def scan_includes(self, root: Path) -> None:
        """Finds the files each package's code names outside its own directory.

        A file its library or binaries embed (include_str!, include_bytes!) is part of
        what they build, so a change to it changes the package. One only its tests embed,
        after a `#[cfg(test)]` or in tests/, is read by them; so is a path its code joins or
        concat!s to CARGO_MANIFEST_DIR outside an include, which is read when the code runs,
        not built in."""
        literal = re.compile(r'include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)')
        from_manifest = re.compile(
            r'(include_(?:str|bytes)!\(\s*)?concat!\(\s*env!\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*,\s*"([^"]+)"\s*\)'
        )
        joined = re.compile(r'env!\(\s*"CARGO_MANIFEST_DIR"\s*\)\s*\)?\s*\.join\(\s*"([^"]+)"\s*\)')
        for name, d in self.dirs.items():
            if d == ".":
                continue
            base = root / d
            for rs in base.rglob("*.rs"):
                if self.package_at(str(PurePosixPath(rs.relative_to(root)))) != name:
                    continue  # a package nested in this one's directory
                try:
                    text = rs.read_text(encoding="utf-8")
                except (OSError, UnicodeDecodeError):
                    continue
                rel = rs.relative_to(base).parts
                in_tests = rel[0] in ("tests", "benches") or rs.stem == "tests" or rs.stem.endswith("_tests")
                # A test module is the end of its file by custom; what follows it is tests'.
                gate = None if in_tests else TEST_GATE.search(text)
                cut = 0 if in_tests else (gate.start() if gate else len(text) + 1)

                def kind(at: int) -> str:
                    return "tests" if at >= cut else "src"

                found = [(rs.parent / m.group(1), kind(m.start())) for m in literal.finditer(text)]
                found += [
                    (base / m.group(2).lstrip("/"), kind(m.start()) if m.group(1) else "tests")
                    for m in from_manifest.finditer(text)
                ]
                found += [(base / m.group(1), "tests") for m in joined.finditer(text)]
                for target, k in found:
                    try:
                        path = str(PurePosixPath(Path(os.path.normpath(target)).relative_to(root)))
                    except ValueError:
                        continue
                    # The whole tree, or the build's own output: what reads it is listed by hand.
                    if path in (".", "target") or path.startswith("target/"):
                        continue
                    if path != d and not path.startswith(d + "/"):
                        self.includes.setdefault(path, set()).add((name, k))

    def includers(self, path: str) -> set[tuple[str, str]]:
        """Who names `path`, itself or a directory holding it."""
        out: set[tuple[str, str]] = set()
        for named, who in self.includes.items():
            if path == named or path.startswith(named + "/"):
                out |= who
        return out

    def package_at(self, path: str) -> str | None:
        """The package whose directory holds `path`, the deepest one if they nest."""
        best = None
        for name, d in self.dirs.items():
            if d != "." and (path == d or path.startswith(d + "/")):
                if best is None or len(d) > len(self.dirs[best]):
                    best = name
        return best

    def dependents(self, names: set[str]) -> set[str]:
        """`names` and every package that depends on one of them, by any kind of dependency."""
        reverse: dict[str, set[str]] = {}
        for name, ds in self.deps.items():
            for d, _ in ds:
                reverse.setdefault(d, set()).add(name)
        seen, todo = set(names), list(names)
        while todo:
            for up in reverse.get(todo.pop(), ()):
                if up not in seen:
                    seen.add(up)
                    todo.append(up)
        return seen

    def linked_by(self, name: str) -> set[str]:
        """The packages a binary or test of `name` links: its own dependencies of any kind,
        and theirs that are not dev-dependencies."""
        seen = {name}
        todo = [d for d, _ in self.deps.get(name, ())]
        while todo:
            d = todo.pop()
            if d in seen:
                continue
            seen.add(d)
            todo.extend(x for x, kind in self.deps.get(d, ()) if kind != "dev")
        return seen


@dataclass
class Changes:
    full: bool = False
    why_full: list[str] = field(default_factory=list)
    crates: set[str] = field(default_factory=set)
    readers: set[str] = field(default_factory=set)
    feature_files: set[str] = field(default_factory=set)
    all_features: bool = False
    # ("import"|"dir"|"tag", value) a crate change selects feature files by
    feature_selectors: set[tuple[str, str]] = field(default_factory=set)
    lint: bool = False
    acceptance: bool = False


def matches(path: str, prefixes) -> str | None:
    for p in prefixes:
        if path == p or (p.endswith("/") and path.startswith(p)):
            return p
    return None


def classify(paths: list[str], ws: Workspace, existing_features: set[str]) -> Changes:
    c = Changes()

    def full(why: str) -> None:
        c.full = True
        c.why_full.append(why)

    for path in paths:
        if f := matches(path, FULL):
            full(f"{path} ({f})")
            continue
        for pkg, kind in ws.includers(path):
            (c.crates if kind == "src" else c.readers).add(pkg)
        if path.endswith(".qml") and not path.startswith("features/"):
            c.readers.update(QML_READERS)
        if path.startswith("acceptance/"):
            c.acceptance = True
            continue
        reader = matches(path, READERS)
        if reader:
            c.readers.update(READERS[reader])
            if reader in FEATURE_READERS:
                c.feature_selectors.add(FEATURE_READERS[reader])
        if path.startswith("features/"):
            c.lint = True
            if path.endswith(".feature"):
                # A deleted file has nothing left to run; the lint still reads the tree.
                if path in existing_features:
                    c.feature_files.add(path)
            else:
                parts = PurePosixPath(path).parts
                if len(parts) > 2:
                    c.feature_selectors.add(("dir", parts[1]))
            continue
        if path == "ledger/" or path.startswith("ledger/"):
            c.lint = True
            continue
        if reader:
            continue
        pkg = ws.package_at(path)
        if pkg:
            if path.endswith(".md"):
                # A crate's notes: its own tests may read them, nothing else does.
                c.readers.add(pkg)
            else:
                c.crates.add(pkg)
            continue
        deps = [d for d in ws.path_deps if path == d or path.startswith(d + "/") or d.startswith(path + "/")]
        if deps:
            for d in deps:
                c.crates.update(ws.path_deps[d])
            continue
        if path.endswith(".md"):
            continue
        if path.startswith(("crates/", "examples/", "vendor/", "third_party/")):
            # Not a package's and not a dependency's: a crate added or removed, a vendored
            # crate only another vendored one uses. Its reach is not known here.
            full(f"{path} (no package or path dependency holds it)")
            continue
        if path.startswith("docs/"):
            continue
        if matches(path, NOTHING):
            continue
        full(f"{path} (not a path these rules know)")
    return c


def feature_weight(text: str) -> int:
    """How many scenarios a feature file runs: each Scenario, and each row of an Examples
    table but its header."""
    weight, in_examples, header = 0, False, False
    for line in text.splitlines():
        s = line.strip()
        if re.match(r"(Scenario|Example)\b", s) and not s.startswith("Examples"):
            if not s.startswith("Scenario Outline") and not s.startswith("Scenario Template"):
                weight += 1
            in_examples = False
        elif s.startswith(("Examples:", "Scenarios:")):
            in_examples, header = True, True
        elif in_examples and s.startswith("|"):
            if header:
                header = False
            else:
                weight += 1
        elif in_examples and s and not s.startswith(("#", "@")):
            in_examples = False
    return max(weight, 1)


def select_features(c: Changes, ws: Workspace, features: dict[str, str]) -> tuple[set[str], list[str]]:
    """The feature files to run, and why each crate selected what it did."""
    if c.full or c.all_features:
        return set(features), []
    notes = []
    linked = ws.linked_by(RUNNER)
    desktop = ws.linked_by(DESKTOP_BIN) - linked
    selectors = set(c.feature_selectors)
    for crate in sorted(c.crates):
        if crate in linked:
            if crate in CONFINED:
                conf = CONFINED[crate]
                selectors.update(("import", m) for m in conf.get("imports", ()))
                selectors.update(("dir", d) for d in conf.get("dirs", ()))
                selectors.update(("tag", t) for t in conf.get("tags", ()))
                notes.append(f"{crate}: its own modules")
            else:
                notes.append(f"{crate}: the runner links it, every feature file")
                return set(features), notes
        elif crate in desktop:
            selectors.add(("tag", "@desktop"))
            notes.append(f"{crate}: @desktop scenarios run its binary")
    chosen = set(c.feature_files)
    for kind, value in selectors:
        if kind == "dir":
            chosen.update(f for f in features if f.startswith(f"features/{value}/"))
        elif kind == "import":
            pattern = re.compile(rf"^\s*import\s+{re.escape(value)}(\s|$)", re.M)
            chosen.update(f for f, text in features.items() if pattern.search(text))
        elif kind == "tag":
            pattern = re.compile(rf"(^|\s){re.escape(value)}(\s|$)", re.M)
            chosen.update(f for f, text in features.items() if pattern.search(text))
    return chosen, notes


def split(weights: dict[str, int], shards: int) -> list[list[str]]:
    """Heaviest first, each onto the lightest shard; ties broken by name, so the split
    is the same for the same tree."""
    bins: list[tuple[int, int, list[str]]] = [(0, i, []) for i in range(shards)]
    for name in sorted(weights, key=lambda n: (-weights[n], n)):
        bins.sort(key=lambda b: (b[0], b[1]))
        w, i, items = bins[0]
        items.append(name)
        bins[0] = (w + weights[name], i, items)
    bins.sort(key=lambda b: b[1])
    return [sorted(items) for _, _, items in bins if items]


def shard_count(total: int, per_shard: int, most: int) -> int:
    return max(1, min(most, math.ceil(total / per_shard)))


def plan(mode: str, paths: list[str], ws: Workspace, features: dict[str, str]) -> tuple[dict, list[str]]:
    c = classify(paths, ws, set(features)) if mode == "pr" else Changes(full=True, why_full=[f"--mode {mode}"])
    log = []
    if c.full:
        log.append("everything: " + "; ".join(c.why_full[:5]))
        tested = set(ws.dirs)
        files = set(features)
        lint = acceptance = True
    else:
        tested = ws.dependents(c.crates) | c.readers
        files, notes = select_features(c, ws, features)
        log.extend(notes)
        lint = c.lint
        acceptance = c.acceptance or bool(c.crates & ws.linked_by(RUNNER))
    built = ws.dependents(c.crates) if not c.full else set(ws.dirs)

    test_shards = []
    if c.full and not ws.dirs:
        # cargo metadata failed: no packages to split, so one shard tests them all.
        test_shards.append({"name": "1/1", "packages": "--workspace", "doc": "--workspace"})
    elif tested:
        weights = {p: max(ws.test_targets.get(p, 0), 1) for p in tested}
        n = shard_count(sum(weights.values()), TEST_TARGETS_PER_SHARD, TEST_SHARDS)
        groups = split(weights, n)
        for i, group in enumerate(groups, 1):
            test_shards.append({
                "name": f"{i}/{len(groups)}",
                "packages": " ".join(f"-p {p}" for p in group),
                "doc": " ".join(f"-p {p}" for p in group if p in ws.doctests),
            })

    feature_shards = []
    if files:
        weights = {f: feature_weight(features[f]) for f in files}
        n = shard_count(sum(weights.values()), SCENARIOS_PER_SHARD, FEATURE_SHARDS)
        groups = split(weights, n)
        for i, group in enumerate(groups, 1):
            feature_shards.append({"name": f"{i}/{len(groups)}", "files": " ".join(group)})

    # Clippy and fmt read what was built from changed code: the crates that changed and
    # those that depend on them. A file only a crate's tests read changes no code.
    clippy_set = built
    out = {
        "full": c.full,
        "clippy": bool(clippy_set),
        "clippy_args": "--workspace" if c.full else " ".join(f"-p {p}" for p in sorted(clippy_set)),
        "wasm": c.full or WASM in clippy_set,
        "tests": test_shards,
        "lint": lint,
        "features": feature_shards,
        "acceptance": acceptance,
    }
    log.append(
        f"crates changed: {', '.join(sorted(c.crates)) or 'none'}; tested: {len(tested)} packages; "
        f"feature files: {len(files)} of {len(features)}"
    )
    return out, log


def git(*args: str, cwd: Path) -> str:
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout


def changed_paths(base: str, head: str, cwd: Path) -> list[str]:
    # Without rename detection a moved file lists both paths: it left one place and reached another.
    out = git("diff", "--no-renames", "--name-only", f"{base}...{head}", cwd=cwd)
    return [line for line in out.splitlines() if line]


def read_features(root: Path, rev: str | None) -> dict[str, str]:
    if rev is None:
        return {
            str(p.relative_to(root)): p.read_text(encoding="utf-8")
            for p in sorted((root / "features").rglob("*.feature"))
        }
    names = git("ls-tree", "-r", "--name-only", rev, "--", "features", cwd=root).split()
    return {n: git("show", f"{rev}:{n}", cwd=root) for n in names if n.endswith(".feature")}


def metadata(root: Path) -> dict:
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"],
        cwd=root, check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def emit(out: dict) -> str:
    lines = []
    for key, value in out.items():
        if isinstance(value, bool):
            value = "true" if value else "false"
        elif not isinstance(value, str):
            value = json.dumps(value, separators=(",", ":"))
        lines.append(f"{key}={value}")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--mode", choices=("pr", "full"), default="pr")
    ap.add_argument("--base", help="the revision the change starts from (its merge base is used)")
    ap.add_argument("--head", default="HEAD")
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    ap.add_argument("--metadata", type=Path, help="cargo metadata JSON, instead of running cargo")
    ap.add_argument("--at-head", action="store_true",
                    help="read feature files at --head through git instead of the working tree")
    args = ap.parse_args(argv)

    if args.mode == "pr" and not args.base:
        ap.error("--mode pr needs --base")
    meta = json.loads(args.metadata.read_text()) if args.metadata else None
    if meta is None:
        try:
            meta = metadata(args.root)
        except (subprocess.CalledProcessError, FileNotFoundError, json.JSONDecodeError) as e:
            # Without the workspace's shape nothing can be left out safely.
            print(f"ci_changes: cargo metadata failed ({e}); running everything", file=sys.stderr)
            args.mode = "full"
            meta = {"workspace_root": str(args.root), "packages": []}
    ws = Workspace.from_metadata(meta)
    ws.scan_includes(args.root)
    features = read_features(args.root, args.head if args.at_head else None)
    paths = changed_paths(args.base, args.head, args.root) if args.mode == "pr" else []
    out, log = plan(args.mode, paths, ws, features)
    for line in log:
        print(f"ci_changes: {line}", file=sys.stderr)
    print(emit(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
