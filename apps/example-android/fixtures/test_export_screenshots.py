#!/usr/bin/env python3
"""Check the actual private export and integration command with host processes."""

from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import export_screenshots as proof

PNG = proof.PNG + b"fixture image"


class ScreenshotExportTest(unittest.TestCase):
    def test_absent_directory_is_empty_only_when_the_owned_app_can_prove_absence(self):
        for accessible in (True, False):
            with (
                self.subTest(accessible=accessible),
                tempfile.TemporaryDirectory() as home,
            ):
                first = subprocess.CalledProcessError(1, ["ls"])
                calls = []

                def owned(serial, *arguments):
                    calls.append(arguments)
                    if arguments[0] == "ls":
                        raise first
                    if arguments[0] == "test" and not accessible:
                        raise subprocess.CalledProcessError(1, ["run-as"])
                    return b""

                with patch.object(proof, "owned", side_effect=owned):
                    if accessible:
                        self.assertEqual(
                            0, proof.export("emulator-fixture", Path(home))
                        )
                    else:
                        with self.assertRaises(
                            subprocess.CalledProcessError
                        ) as failure:
                            proof.export("emulator-fixture", Path(home))
                        self.assertIs(first, failure.exception)
                self.assertEqual(("test", "!", "-e", proof.PRIVATE), calls[1])
                self.assertEqual(("rm", "-rf", proof.PRIVATE), calls[-1])

    def test_only_named_fixture_files_are_exported_then_private_files_are_removed(self):
        calls = []
        with tempfile.TemporaryDirectory() as home:
            output = Path(home)
            (output / "setup.png").write_bytes(b"old screenshot")
            (output / "unrelated.txt").write_text("keep")

            def owned(serial, *arguments):
                calls.append(arguments)
                if arguments[0] == "ls":
                    return b"scale-start.png\n"
                return PNG if arguments[0] == "head" else b""

            with patch.object(proof, "owned", side_effect=owned):
                self.assertEqual(1, proof.export("emulator-fixture", output))
            self.assertEqual(PNG, (output / "scale-start.png").read_bytes())
            self.assertFalse((output / "setup.png").exists())
            self.assertEqual("keep", (output / "unrelated.txt").read_text())
            self.assertEqual(
                (
                    "head",
                    "-c",
                    str(proof.LIMIT + 1),
                    "files/xmtp-messenger-proof/scale-start.png",
                ),
                calls[1],
            )
            self.assertEqual(("rm", "-rf", "files/xmtp-messenger-proof"), calls[-1])

    def test_unknown_filename_is_never_read_or_exported(self):
        calls = []
        with tempfile.TemporaryDirectory() as home:

            def owned(serial, *arguments):
                calls.append(arguments)
                return b"profile.json\n" if arguments[0] == "ls" else PNG

            with patch.object(proof, "owned", side_effect=owned):
                with self.assertRaisesRegex(ValueError, "Unknown or duplicate"):
                    proof.export("emulator-fixture", Path(home))
            self.assertFalse(any(call[0] == "head" for call in calls))
            self.assertFalse((Path(home) / "profile.json").exists())
            self.assertEqual(("rm", "-rf", proof.PRIVATE), calls[-1])


if __name__ == "__main__":
    unittest.main()
