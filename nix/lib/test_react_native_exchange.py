"""Admission checks for same-run immutable product transport."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "rn_exchange", Path(__file__).resolve().parents[1] / "react-native-exchange.py"
)
exchange = importlib.util.module_from_spec(spec)
spec.loader.exec_module(exchange)


class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        self.cache = self.root / "cache"
        (self.cache / "nar").mkdir(parents=True)
        self.product = "/nix/store/" + "a" * 32 + "-expected-rn-package"
        self.expected = {"xmtp-sdk-react-native-package": self.product}
        self.nar = self.cache / "nar/test.nar.xz"
        self.nar.write_bytes(b"exported NAR bytes")
        self.narinfo = self.cache / ("a" * 32 + ".narinfo")
        self.narinfo.write_text(f"StorePath: {self.product}\nURL: nar/test.nar.xz\n")
        self.manifest = {
            "schema": 2,
            "system": "x86_64-linux",
            "stage": "package",
            "roots": self.expected.copy(),
            "publicKey": "rn-exchange-" + "b" * 32 + ":" + "A" * 43 + "=",
            "cacheFiles": exchange.inventory(self.cache),
            "closure": {self.product: {"narHash": "sha256-test", "references": []}},
        }

    def invoke(self):
        (self.root / "manifest.json").write_text(json.dumps(self.manifest))
        return [
            "exchange",
            "import",
            "--system",
            "x86_64-linux",
            "--stage",
            "package",
            "--directory",
            str(self.root),
        ]

    def rejected(self, message):
        with (
            patch("sys.argv", self.invoke()),
            patch.object(exchange, "roots", return_value=self.expected),
            patch.object(exchange, "info", return_value=self.manifest["closure"]),
            patch.object(exchange, "run") as process,
        ):
            with self.assertRaisesRegex(ValueError, message):
                exchange.main()
            process.assert_not_called()

    def test_other_revision_product_is_rejected_before_import(self):
        self.manifest["roots"] = {
            "xmtp-sdk-react-native-package": self.product + "-other-revision"
        }
        self.rejected("declared RN products")

    def test_changed_nar_is_rejected_before_import(self):
        self.nar.write_bytes(b"tampered export")
        self.rejected("cache file hashes")

    def test_unreachable_store_entry_is_rejected_before_import(self):
        unrelated = "/nix/store/" + "c" * 32 + "-unrelated"
        self.manifest["closure"][unrelated] = {
            "narHash": "sha256-other",
            "references": [],
        }
        (self.cache / ("c" * 32 + ".narinfo")).write_text(
            f"StorePath: {unrelated}\nURL: nar/test.nar.xz\n"
        )
        self.manifest["cacheFiles"] = exchange.inventory(self.cache)
        self.rejected("undeclared unreachable paths")

    def test_narinfo_cannot_fetch_outside_the_artifact(self):
        (self.root / "outside.nar").write_bytes(b"outside the artifact cache")
        self.narinfo.write_text(f"StorePath: {self.product}\nURL: ../outside.nar\n")
        self.manifest["cacheFiles"] = exchange.inventory(self.cache)
        self.rejected("local NAR file")

    def test_wrong_signature_stops_before_destination_copy(self):
        commands = []

        def process(*args, **_kwargs):
            commands.append(args)
            if args[:3] == ("nix", "store", "verify"):
                raise subprocess.CalledProcessError(1, args)
            if args[:2] == ("nix", "copy"):
                return subprocess.CompletedProcess(args, 0)
            raise AssertionError("unexpected command")

        with (
            patch("sys.argv", self.invoke()),
            patch.object(exchange, "roots", return_value=self.expected),
            patch.object(exchange, "info", return_value=self.manifest["closure"]),
            patch.object(exchange, "run", side_effect=process),
        ):
            with self.assertRaises(subprocess.CalledProcessError):
                exchange.main()
        self.assertEqual(len(commands), 1)
        self.assertIn("trusted-public-keys", commands[0])


if __name__ == "__main__":
    unittest.main()
