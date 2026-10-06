#!/usr/bin/env python3
"""Check selected artifact scopes and the retained aggregate receipt."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from packaging_test_base import receipt

LANGUAGES = ("swift", "kotlin", "typescript-napi", "typescript-wasm", "typescript-pure")


class GeneratedRecords(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        self.generated = self.root / "generated"
        self.generated.mkdir()
        self.binaries = {}
        for role in ("native", "bindgen", "wasm", "pure"):
            binary = self.root / f"{role}.bin"
            binary.write_bytes(role.encode())
            self.binaries[role] = binary
        self.identity = patch.object(
            receipt.artifacts,
            "source_hash",
            side_effect=lambda generator=False: (
                "generator-source" if generator else "native-source"
            ),
        )
        self.identity.start()

    def tearDown(self):
        self.identity.stop()
        self.temporary.cleanup()

    def trees(self, languages):
        for language in languages:
            tree = self.generated / language
            tree.mkdir()
            (tree / "binding.txt").write_text(language)

    def read(self, language):
        return json.loads((self.generated / language / "sdk-contract.json").read_text())

    def test_each_native_language_requires_only_native_and_bindgen(self):
        for language in LANGUAGES[:3]:
            with self.subTest(language=language):
                self.generated = self.root / language
                self.generated.mkdir()
                self.trees([language])
                selected = {role: self.binaries[role] for role in ("native", "bindgen")}
                receipt.record(self.generated, selected)
                record = self.read(language)
                self.assertEqual(record["artifact"]["source"], "native-source")
                self.assertEqual(record["generator"], "generator-source")
                self.assertEqual(
                    record["artifact"]["files"],
                    {
                        str(self.binaries["native"]): hashlib.sha256(
                            b"native"
                        ).hexdigest(),
                    },
                )
                self.assertEqual(
                    record["files"],
                    {
                        "binding.txt": hashlib.sha256(language.encode()).hexdigest(),
                    },
                )

    def test_browser_cli_accepts_no_native_and_records_matched_roles(self):
        self.trees(LANGUAGES[3:])
        arguments = ["record-generated.py", str(self.generated)]
        for role in ("bindgen", "wasm", "pure"):
            arguments.extend([f"--{role}", str(self.binaries[role])])
        with patch("sys.argv", arguments):
            receipt.main()
        worker = self.read("typescript-wasm")
        pure = self.read("typescript-pure")
        self.assertEqual(worker["contract"], pure["contract"])
        self.assertEqual(worker["artifact"]["features"], "")
        self.assertEqual(pure["artifact"]["features"], "pure-only")
        for language, role in zip(LANGUAGES[3:], ("wasm", "pure")):
            record = self.read(language)
            self.assertEqual(
                record["artifact"]["files"],
                {
                    str(self.binaries[role]): hashlib.sha256(role.encode()).hexdigest(),
                },
            )

    def test_missing_artifact_roles_fail_before_receipts_are_written(self):
        self.trees(LANGUAGES)
        for role in self.binaries:
            with self.subTest(role=role):
                selected = {
                    name: path for name, path in self.binaries.items() if name != role
                }
                with self.assertRaisesRegex(ValueError, f"requires {role} artifact"):
                    receipt.record(self.generated, selected)
                self.assertFalse(any(self.generated.rglob("sdk-contract.json")))

    def test_aggregate_keeps_full_role_contract_and_record_schema(self):
        self.trees(LANGUAGES)
        receipt.record(self.generated, self.binaries)
        # This digest comes from the prior aggregate producer and fixture bytes.
        expected = "06e740082b026dc6481cf9a2749667234c0d9465d70e43cbe32891881e8815e5"
        for language in LANGUAGES:
            record = self.read(language)
            self.assertEqual(record["contract"], expected)
            self.assertEqual(
                set(record), {"contract", "generator", "artifact", "files"}
            )
            self.assertEqual(
                set(record["artifact"]),
                {
                    "source",
                    "generator",
                    "features",
                    "profile",
                    "target",
                    "files",
                },
            )
        receipt.record(self.generated, self.binaries)
        self.assertEqual(
            self.read("swift")["files"],
            {
                "binding.txt": hashlib.sha256(b"swift").hexdigest(),
            },
        )


if __name__ == "__main__":
    unittest.main()
