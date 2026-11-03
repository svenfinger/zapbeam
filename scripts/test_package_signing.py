"""Release signature policy tests; native tools are exercised by the macOS job."""
import importlib.util
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("packages", Path(__file__).with_name("verify-packages.py"))
packages = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packages)
IDENTITY = "Developer ID Application: Example (ABCDEFGHIJ)"
DETAILS = f"""Authority={IDENTITY}
TeamIdentifier=ABCDEFGHIJ
Identifier=io.github.svenfinger.zapbeam
Timestamp=Nov 3, 2026
CodeDirectory v=20500 flags=0x10000(runtime)
"""


class SigningTests(unittest.TestCase):
    def verify(self, details):
        with patch.object(packages, "run", side_effect=["", details]):
            packages.verify_signature(Path("Example.app"), IDENTITY, "ABCDEFGHIJ",
                                      identifier="io.github.svenfinger.zapbeam", runtime=True)

    def test_accept_expected_signature(self):
        self.verify(DETAILS)

    def test_reject_policy_mismatches(self):
        for before, after in ((IDENTITY, "Developer ID Application: Other (ABCDEFGHIJ)"),
                              ("TeamIdentifier=ABCDEFGHIJ", "TeamIdentifier=ZZZZZZZZZZ"),
                              ("io.github.svenfinger.zapbeam", "wrong.identifier"),
                              ("Timestamp=", "NoTimestamp="),
                              ("(runtime)", "(adhoc)")):
            with self.subTest(before=before), self.assertRaises(RuntimeError):
                self.verify(DETAILS.replace(before, after))
        with self.assertRaises(RuntimeError):
            self.verify(DETAILS + "Signature=adhoc\n")

    def test_missing_resource_seal_is_rejected(self):
        with patch.dict("os.environ", {"APPLE_SIGNING_IDENTITY": IDENTITY}):
            with patch.object(Path, "is_file", return_value=False):
                with self.assertRaisesRegex(RuntimeError, "resource seal"):
                    packages.verify_macos_release(Path("Example.app"), Path("Example.dmg"))

    def test_gatekeeper_and_ticket_failures_propagate(self):
        for failed_tool in ("spctl", "xcrun"):
            def run(*args):
                if args[0] == failed_tool:
                    raise subprocess.CalledProcessError(1, args)
                return ""
            with self.subTest(tool=failed_tool), patch.dict("os.environ", {"APPLE_SIGNING_IDENTITY": IDENTITY}):
                with patch.object(Path, "is_file", return_value=True), patch.object(Path, "rglob", return_value=[]):
                    with patch.object(packages, "verify_signature"), patch.object(packages, "run", side_effect=run):
                        with self.assertRaises(subprocess.CalledProcessError):
                            packages.verify_macos_release(Path("Example.app"), Path("Example.dmg"))

    def test_native_verification_failure_propagates(self):
        with patch.object(packages, "run", side_effect=subprocess.CalledProcessError(1, "codesign")):
            with self.assertRaises(subprocess.CalledProcessError):
                packages.verify_signature(Path("Broken.app"), IDENTITY, "ABCDEFGHIJ")


if __name__ == "__main__":
    unittest.main()
