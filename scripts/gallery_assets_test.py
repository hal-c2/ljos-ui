#!/usr/bin/env python3
"""Tests for gallery-assets.py: python3 scripts/gallery_assets_test.py"""

import os
import runpy
import tempfile
import unittest
from pathlib import Path

SCRIPT = str(Path(__file__).resolve().parent / "gallery-assets.py")


def places(environ):
    """TARGET and CACHE as the script reads them, run from a directory of its own."""
    saved_env, saved_cwd = dict(os.environ), os.getcwd()
    with tempfile.TemporaryDirectory() as here:
        try:
            os.chdir(here)
            os.environ.pop("CARGO_TARGET_DIR", None)
            os.environ.pop("LJOS_CORPUS_ASSETS", None)
            os.environ.update(environ)
            found = runpy.run_path(SCRIPT, run_name="gallery_assets")
            return os.path.realpath(here), found["TARGET"], found["CACHE"]
        finally:
            os.chdir(saved_cwd)
            os.environ.clear()
            os.environ.update(saved_env)


class Places(unittest.TestCase):
    # The assets are linked into corpus/qtdoc: a link to a relative path
    # points from the example's directory, where nothing was downloaded.
    def test_a_relative_cache_is_where_it_was_named(self):
        here, _, cache = places({"LJOS_CORPUS_ASSETS": "assets"})
        self.assertTrue(os.path.isabs(cache), cache)
        self.assertEqual(os.path.realpath(cache), os.path.join(here, "assets"))

    def test_a_relative_target_is_where_it_was_named(self):
        here, target, cache = places({"CARGO_TARGET_DIR": "tgt"})
        self.assertTrue(os.path.isabs(target), target)
        self.assertEqual(os.path.realpath(target), os.path.join(here, "tgt"))
        self.assertEqual(os.path.realpath(cache), os.path.join(here, "tgt", "corpus-assets"))


if __name__ == "__main__":
    unittest.main()
