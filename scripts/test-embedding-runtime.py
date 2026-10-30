#!/usr/bin/env python3
"""Offline package-pin regression checks. Stage a package before running."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('stage', ROOT/'scripts/stage-embedding-runtime.py')
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)


class PackagePin(unittest.TestCase):
    def setUp(self):
        self.pin = json.loads((ROOT/'src-tauri/runtime/embedding-macos-arm64.json').read_text())
        self.package = ROOT/'build/embedding-runtime/resources/embedding-runtime.zip'

    def test_actual_package_matches_every_compiled_file_pin(self):
        stage.verify(self.package, self.pin)

    def test_wrong_archive_pin_is_rejected(self):
        bad = copy.deepcopy(self.pin)
        bad['archiveSha256'] = '0'*64
        with self.assertRaises(ValueError):
            stage.verify(self.package, bad)

    def test_missing_inventory_entry_is_rejected(self):
        bad = copy.deepcopy(self.pin)
        del bad['files']['llama-server']
        with self.assertRaises(ValueError):
            stage.verify(self.package, bad)

    def test_wrong_file_hash_is_rejected_despite_correct_archive(self):
        bad = copy.deepcopy(self.pin)
        bad['files']['libllama.0.6.0.dylib']['sha256'] = '0'*64
        with self.assertRaises(ValueError):
            stage.verify(self.package, bad)

    def test_truncated_archive_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            truncated = Path(directory)/'bad.zip'
            truncated.write_bytes(self.package.read_bytes()[:-1])
            with self.assertRaises(ValueError):
                stage.verify(truncated, self.pin)


if __name__ == '__main__':
    unittest.main()
