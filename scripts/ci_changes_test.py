#!/usr/bin/env python3
"""Tests for ci_changes.py on a made-up workspace: python3 scripts/ci_changes_test.py"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import ci_changes as cc  # noqa: E402

ROOT = "/w"


def package(name, deps=(), tests=1, doctest=True, path=None):
    """`deps` are names, or (name, kind) pairs, or ("path", dir) for a path outside members."""
    dependencies = []
    for d in deps:
        if isinstance(d, tuple) and d[0] == "path":
            dependencies.append({"name": d[1].rsplit("/", 1)[-1], "path": f"{ROOT}/{d[1]}"})
        elif isinstance(d, tuple):
            dependencies.append({"name": d[0], "kind": d[1], "path": f"{ROOT}/crates/{d[0]}"})
        else:
            dependencies.append({"name": d, "kind": None, "path": f"{ROOT}/crates/{d}"})
    targets = [{"kind": ["lib"], "test": True, "doctest": doctest}]
    targets += [{"kind": ["test"], "test": True}] * (tests - 1)
    return {
        "name": name,
        "manifest_path": f"{ROOT}/{path or 'crates/' + name}/Cargo.toml",
        "dependencies": dependencies,
        "targets": targets,
    }


# The engine underneath; the runner and the desktop binary on top; a confined module crate
# that only the runner links; a leaf crate nothing links.
META = {
    "workspace_root": ROOT,
    "packages": [
        package("ljos_engine", deps=[("path", "vendor/rquickjs")], tests=5),
        package("ljos_quick", deps=["ljos_engine"], tests=3),
        package("ljos_quick3d", deps=["ljos_quick"]),
        package("ljos_bdd", deps=["ljos_quick", "ljos_quick3d"], tests=2),
        package("ljos_cli", deps=["ljos_quick", ("ljos_bdd", "dev")]),
        package("ljos_mcp_author", deps=["ljos_quick"]),
        package("ljos_mcp_core"),
        package("qml_fmt", doctest=False),
    ],
}

FEATURES = {
    "features/quick/text.feature": "Feature: t\n  Scenario: a\n  Scenario: b\n",
    "features/quick3d/view.feature": 'Feature: v\n  Scenario: a\n    Given """\n    import QtQuick3D\n    """\n',
    "features/quick/uses3d.feature": "Feature: u\n  Scenario: a\n    Given\n      import QtQuick3D 6.0\n",
    "features/desktop/window.feature": "@desktop\nFeature: w\n  Scenario: a\n",
    "features/quick/outline.feature": (
        "Feature: o\n  Scenario Outline: x <n>\n    Examples:\n      | n |\n      | 1 |\n      | 2 |\n      | 3 |\n"
    ),
}


def run(*paths, mode="pr", features=FEATURES):
    out, _ = cc.plan(mode, list(paths), cc.Workspace.from_metadata(META), features)
    return out


def tested(out):
    return {p.removeprefix("-p ") for s in out["tests"] for p in s["packages"].split(" -p ")}


def feature_files(out):
    return {f for s in out["features"] for f in s["files"].split()}


class Plans(unittest.TestCase):
    def test_docs_run_nothing(self):
        out = run("docs/ARCHITECTURE.md", "README.md", "docs/site/index.html")
        self.assertFalse(out["full"] or out["clippy"] or out["lint"] or out["acceptance"])
        self.assertEqual(out["tests"], [])
        self.assertEqual(out["features"], [])

    def test_engine_tests_its_dependents_and_every_feature(self):
        out = run("crates/ljos_engine/src/model.rs")
        self.assertFalse(out["full"])
        self.assertEqual(tested(out), set(cc.Workspace.from_metadata(META).dirs) - {"ljos_mcp_core", "qml_fmt"})
        self.assertEqual(feature_files(out), set(FEATURES))
        self.assertTrue(out["acceptance"])
        self.assertIn("-p ljos_engine", out["clippy_args"])
        self.assertFalse(out["wasm"])

    def test_a_feature_file_runs_alone_with_the_lint(self):
        out = run("features/quick/text.feature")
        self.assertEqual(feature_files(out), {"features/quick/text.feature"})
        self.assertTrue(out["lint"])
        self.assertFalse(out["clippy"])
        self.assertEqual(out["tests"], [])

    def test_a_deleted_feature_file_runs_only_the_lint(self):
        out = run("features/quick/gone.feature")
        self.assertEqual(out["features"], [])
        self.assertTrue(out["lint"])

    def test_a_fixture_runs_its_area(self):
        out = run("features/quick/fixtures/logo.png")
        self.assertEqual(
            feature_files(out),
            {"features/quick/text.feature", "features/quick/uses3d.feature", "features/quick/outline.feature"},
        )

    def test_the_lockfile_runs_everything(self):
        for path in ("Cargo.lock", "mise.toml", ".github/workflows/ci.yml", "crates/ljos_bdd/src/steps.rs"):
            with self.subTest(path):
                out = run(path)
                self.assertTrue(out["full"])
                self.assertEqual(out["clippy_args"], "--workspace")
                self.assertEqual(feature_files(out), set(FEATURES))
                self.assertTrue(out["wasm"] and out["lint"] and out["acceptance"])

    def test_an_unknown_path_runs_everything(self):
        self.assertTrue(run("tools/new-thing.sh")["full"])
        self.assertTrue(run("crates/ljos_new/src/lib.rs")["full"])

    def test_a_confined_crate_runs_the_files_importing_its_module(self):
        out = run("crates/ljos_quick3d/src/lib.rs")
        self.assertEqual(feature_files(out), {"features/quick3d/view.feature", "features/quick/uses3d.feature"})
        self.assertEqual(tested(out), {"ljos_quick3d", "ljos_bdd", "ljos_cli"})

    def test_a_crate_only_the_desktop_binary_links_runs_desktop_scenarios(self):
        meta = json.loads(json.dumps(META))
        meta["packages"].append(package("ljos_shell"))
        next(p for p in meta["packages"] if p["name"] == "ljos_cli")["dependencies"].append(
            {"name": "ljos_shell", "kind": None, "path": f"{ROOT}/crates/ljos_shell"}
        )
        out, _ = cc.plan("pr", ["crates/ljos_shell/src/lib.rs"], cc.Workspace.from_metadata(meta), FEATURES)
        self.assertEqual(feature_files(out), {"features/desktop/window.feature"})
        self.assertFalse(out["acceptance"])

    def test_a_leaf_crate_tests_itself(self):
        out = run("crates/qml_fmt/src/lib.rs")
        self.assertEqual(tested(out), {"qml_fmt"})
        self.assertEqual(out["features"], [])
        self.assertEqual(out["tests"][0]["doc"], "")

    def test_the_web_crate_checks_the_web_build(self):
        self.assertTrue(run("crates/ljos_mcp_core/src/lib.rs")["wasm"])

    def test_a_file_a_crate_reads_tests_that_crate_only(self):
        out = run("skills/embedding/SKILL.md")
        self.assertEqual(tested(out), {"ljos_mcp_author"})
        self.assertFalse(out["clippy"])

    def test_a_crates_notes_test_that_crate_only(self):
        out = run("crates/ljos_engine/README.md")
        self.assertEqual(tested(out), {"ljos_engine"})
        self.assertFalse(out["clippy"])

    def test_a_vendored_path_dependency_tests_its_users(self):
        out = run("vendor/rquickjs/src/lib.rs")
        self.assertIn("ljos_engine", tested(out))
        self.assertFalse(out["full"])
        self.assertTrue(run("vendor/unused/src/lib.rs")["full"])

    def test_acceptance_alone(self):
        out = run("acceptance/example/suite.toml")
        self.assertTrue(out["acceptance"])
        self.assertEqual(out["tests"], [])

    def test_full_mode_runs_everything(self):
        out = run(mode="full")
        self.assertTrue(out["full"])
        self.assertEqual(tested(out), set(cc.Workspace.from_metadata(META).dirs))

    def test_full_mode_without_metadata_tests_the_workspace(self):
        out, _ = cc.plan("full", [], cc.Workspace.from_metadata({"workspace_root": ROOT, "packages": []}), FEATURES)
        self.assertEqual(out["tests"], [{"name": "1/1", "packages": "--workspace", "doc": "--workspace"}])


    def test_qml_anywhere_tests_the_tree_wide_qml_readers(self):
        meta = json.loads(json.dumps(META))
        for name in cc.QML_READERS:
            meta["packages"].append(package(name))
        ws = cc.Workspace.from_metadata(meta)
        for path in ("crates/ljos_quick/qml/Button.qml", "bench/form.qml", "examples/hello/main.qml"):
            with self.subTest(path):
                out, _ = cc.plan("pr", [path], ws, FEATURES)
                self.assertLessEqual(set(cc.QML_READERS), tested(out))
        out, _ = cc.plan("pr", ["features/quick/fixtures/a.qml"], ws, FEATURES)
        self.assertFalse(set(cc.QML_READERS) & tested(out))

    def test_an_embedded_file_changes_the_crate_that_embeds_it(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            meta = json.loads(json.dumps(META).replace(ROOT, tmp))
            # The binary that serves the brief.
            next(p for p in meta["packages"] if p["name"] == "ljos_cli")["dependencies"].append(
                {"name": "ljos_mcp_author", "kind": None, "path": f"{tmp}/crates/ljos_mcp_author"}
            )
            for rel, text in {
                "crates/ljos_mcp_author/src/prompts.rs": 'const B: &str = include_str!("../../../docs/design/brief.md");',
                "crates/ljos_engine/tests/manifest.rs": 'const L: &str = include_str!("../../../ledger/x.toml");',
                "crates/ljos_quick/src/own.rs": 'const O: &str = include_str!("own.txt");',
                "crates/ljos_quick/src/model.rs": (
                    'fn a() {}\n#[cfg(all(test, feature = "decode"))]\nmod tests { const C: &[u8] = include_bytes!("../../../docs/media/clip.h264"); }'
                ),
                "crates/qml_fmt/src/tests.rs": (
                    'const D: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../ljos_quick/demo/Main.qml"));'
                ),
                "crates/ljos_engine/src/lib.rs": 'fn f() { Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/x"); }',
            }.items():
                (root / rel).parent.mkdir(parents=True, exist_ok=True)
                (root / rel).write_text(text)
            ws = cc.Workspace.from_metadata(meta)
            ws.scan_includes(root)
            self.assertEqual(ws.includes, {
                "docs/design/brief.md": {("ljos_mcp_author", "src")},
                "ledger/x.toml": {("ljos_engine", "tests")},
                "docs/media/clip.h264": {("ljos_quick", "tests")},
                "crates/ljos_quick/demo/Main.qml": {("qml_fmt", "tests")},
                "corpus/x": {("ljos_engine", "tests")},
            })
            out, _ = cc.plan("pr", ["docs/media/clip.h264"], ws, FEATURES)
            self.assertEqual(tested(out), {"ljos_quick"})
            self.assertFalse(out["clippy"])
            out, _ = cc.plan("pr", ["corpus/x/main.qml"], ws, FEATURES)
            self.assertIn("ljos_engine", tested(out))
            out, _ = cc.plan("pr", ["docs/design/brief.md"], ws, FEATURES)
            self.assertIn("ljos_cli", tested(out))
            self.assertTrue(out["clippy"])
            out, _ = cc.plan("pr", ["ledger/x.toml"], ws, FEATURES)
            self.assertIn("ljos_engine", tested(out))
            self.assertNotIn("ljos_quick", tested(out))


class Shards(unittest.TestCase):
    def test_weights_count_scenarios_and_example_rows(self):
        self.assertEqual(cc.feature_weight(FEATURES["features/quick/text.feature"]), 2)
        self.assertEqual(cc.feature_weight(FEATURES["features/quick/outline.feature"]), 3)

    def test_the_split_is_balanced_and_stable(self):
        weights = {f"f{i}": w for i, w in enumerate([9, 7, 6, 5, 4, 3, 2, 1, 1])}
        a = cc.split(weights, 3)
        self.assertEqual(a, cc.split(dict(reversed(list(weights.items()))), 3))
        loads = [sum(weights[n] for n in s) for s in a]
        self.assertLessEqual(max(loads) - min(loads), 2)
        self.assertEqual(sorted(n for s in a for n in s), sorted(weights))

    def test_shard_count_is_bounded(self):
        self.assertEqual(cc.shard_count(1, 1500, 6), 1)
        self.assertEqual(cc.shard_count(15_000, 1500, 6), 6)
        self.assertEqual(cc.shard_count(3000, 1500, 6), 2)

    def test_output_is_one_line_per_key(self):
        text = cc.emit(run("crates/ljos_engine/src/model.rs"))
        keys = [line.split("=", 1)[0] for line in text.splitlines()]
        self.assertEqual(keys, ["full", "clippy", "clippy_args", "wasm", "tests", "lint", "features", "acceptance"])
        json.loads(dict(line.split("=", 1) for line in text.splitlines())["tests"])


if __name__ == "__main__":
    unittest.main()
