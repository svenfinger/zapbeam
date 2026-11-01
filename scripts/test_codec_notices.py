"""Regression checks for notice collection and safe output replacement."""

import runpy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("codec-notices.py")


class NoticeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.module = runpy.run_path(str(SCRIPT))
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.destination = self.root / "RUST.txt"
        self.destination.write_text("previous notices", encoding="utf-8")

    def packages(self) -> list[dict[str, str]]:
        packages = []
        for name in sorted(self.module["CODEC_NAMES"]):
            directory = self.root / name
            directory.mkdir()
            (directory / "LICENSE").write_text("Copyright © Example", encoding="utf-8")
            packages.append(
                {
                    "name": name,
                    "version": "1.0.0",
                    "license": "MIT",
                    "manifest_path": str(directory / "Cargo.toml"),
                }
            )
        return packages

    def generate(self, packages: list[dict[str, str]]) -> None:
        contents = self.module["collect_notices"](self.root, packages)
        self.module["write_notices"](self.destination, contents)

    def assert_preserved(self) -> None:
        self.assertEqual(
            self.destination.read_text(encoding="utf-8"), "previous notices"
        )
        self.assertEqual(list(self.root.glob(".RUST.txt.*")), [])

    def test_missing_package_preserves_output(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "Missing packages:"):
            self.generate(self.packages()[1:])
        self.assert_preserved()

    def test_missing_notice_preserves_output(self) -> None:
        packages = self.packages()
        (self.root / "image/LICENSE").unlink()
        with self.assertRaisesRegex(RuntimeError, "Missing notices: image"):
            self.generate(packages)
        self.assert_preserved()

    def test_invalid_encoding_preserves_output(self) -> None:
        packages = self.packages()
        (self.root / "image/LICENSE").write_bytes(b"\xff")
        with self.assertRaises(UnicodeDecodeError):
            self.generate(packages)
        self.assert_preserved()

    def test_replace_failure_cleans_up_and_preserves_output(self) -> None:
        with (
            patch.object(Path, "replace", side_effect=OSError("replacement failed")),
            self.assertRaisesRegex(OSError, "replacement failed"),
        ):
            self.generate(self.packages())
        self.assert_preserved()

    def test_output_is_sorted_utf8_and_preserves_permissions(self) -> None:
        packages = self.packages()
        self.destination.chmod(0o640)
        self.generate(list(reversed(packages)))
        contents = self.destination.read_text(encoding="utf-8")
        self.assertIn("Copyright © Example", contents)
        headings = [line for line in contents.splitlines() if line.startswith("=====")]
        self.assertEqual(headings, sorted(headings))
        self.assertEqual(self.destination.stat().st_mode & 0o777, 0o640)
        self.assertEqual(list(self.root.glob(".RUST.txt.*")), [])

    def test_zune_fallback(self) -> None:
        packages = self.packages()
        (self.root / "zune-core/LICENSE").unlink()
        fallback = self.root / "docs/licenses/ZUNE-MIT.txt"
        fallback.parent.mkdir(parents=True)
        fallback.write_text("Zune fallback notice", encoding="utf-8")
        self.generate(packages)
        self.assertIn(
            "Zune fallback notice", self.destination.read_text(encoding="utf-8")
        )


if __name__ == "__main__":
    unittest.main()
