#!/usr/bin/env python3
"""Tests for the actual Python verification/packaging tools, NOT for Rust behavior."""
import tempfile
import unittest
from pathlib import Path, PurePosixPath
import zipfile
import audit_abi
import package_source
import source_checks


class AbiTests(unittest.TestCase):
    def test_versions_sorted_numerically(self):
        values = audit_abi.requirements("GLIBC_2.9 GLIBC_2.17 GLIBC_2.28 GLIBCXX_3.4.21")
        self.assertEqual(values["GLIBC"], ["2.9", "2.17", "2.28"])
        self.assertEqual(audit_abi.exceeds(values["GLIBC"], "2.17"), ["2.28"])

    def test_separate_symbol_families(self):
        self.assertEqual(audit_abi.requirements("GLIBCXX_3.4.30")["GLIBC"], [])

    def test_equal_baseline_is_allowed(self):
        self.assertEqual(audit_abi.exceeds(["2.2.5", "2.17"], "2.17"), [])


class ArchiveTests(unittest.TestCase):
    def test_runtime_and_font_files_are_excluded(self):
        for name in ["target/a", ".git/config", "x/__pycache__/a.pyc", "docs/a.ttf"]:
            self.assertFalse(package_source.allowed(PurePosixPath(name)))
        self.assertTrue(package_source.allowed(PurePosixPath("src/main.rs")))

    def test_zip_roundtrip(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            root = base / "source"
            root.mkdir()
            (root / "README.md").write_text("中文 source\n", encoding="utf-8")
            output = base / "test.zip"
            result = package_source.package(root, output)
            self.assertEqual(result["files"], 2)
            package_source.verify(output)

    def test_bad_paths_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "bad.zip"
            with zipfile.ZipFile(output, "w") as archive:
                archive.writestr("../escape", "bad")
            with self.assertRaises(ValueError):
                package_source.verify(output)

    def test_corrupted_manifest_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "bad.zip"
            with zipfile.ZipFile(output, "w") as archive:
                archive.writestr("devhub/a", "data")
                archive.writestr("devhub/MANIFEST.sha256", "0" * 64 + "  a\n")
            with self.assertRaises(ValueError):
                package_source.verify(output)

    def test_missing_required_files_detected(self):
        with tempfile.TemporaryDirectory() as temporary:
            self.assertTrue(source_checks.inspect(Path(temporary))["failed"])


class LexerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        try:
            import pygments
        except ImportError:
            raise unittest.SkipTest("optional Pygments is not installed")

    def test_braces_in_comments_and_literals_ignored(self):
        self.assertEqual(source_checks.rust_delimiters('fn x() { let s = "}"; /* { */ }'), [])

    def test_missing_delimiter_is_detected(self):
        self.assertTrue(source_checks.rust_delimiters("fn x() { (1 + 2] }"))

    def test_lifetimes_and_raw_strings(self):
        self.assertEqual(source_checks.rust_delimiters('fn x<\'a>(s: &\'a str) { let r = r#"}[{"#; }'), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
