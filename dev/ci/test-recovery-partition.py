#!/usr/bin/env python3
"""Protect recovery case ownership at the real command entry point."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "dev/ci/recovery-partition.py"
spec = importlib.util.spec_from_file_location("recovery_partition", TOOL)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

# This independent inventory is the existing public matrix.
PAIRS = [
    ("callback", "disconnect"),
    ("iterator", "disconnect"),
    ("callback", "blackhole-inbound"),
    ("iterator", "blackhole-inbound"),
    ("callback", "blackhole-outbound"),
    ("iterator", "blackhole-outbound"),
    ("callback", "drain"),
    ("iterator", "drain"),
]
NAMES = [
    f"receives and replies through state changes in {mode} mode after {fault} (drain: graceful backend restart)"
    for mode, fault in PAIRS
]


class RecoveryPartitionTests(unittest.TestCase):
    def check_inventory(self, names):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "inventory.json"
            path.write_text(
                json.dumps(
                    [
                        {
                            "file": str(ROOT / "sdks/node/test/streamRecovery.test.ts"),
                            "name": "public message stream recovery > " + name,
                        }
                        for name in names
                    ]
                )
            )
            return subprocess.run(
                [sys.executable, str(TOOL), "check", "--inventory", str(path)],
                capture_output=True,
                text=True,
            )

    def test_existing_matrix_has_exact_owners(self):
        result = self.check_inventory(NAMES)
        self.assertEqual(result.returncode, 0, result.stderr)
        groups = module.case_map(ROOT / "dev/ci/recovery-cases.json")
        self.assertEqual(groups["A"], [NAMES[4], NAMES[6]])
        self.assertEqual(groups["B"], [NAMES[5], NAMES[7]])
        self.assertEqual(groups["C"], NAMES[:4])

    def test_new_case_fails_before_execution(self):
        result = self.check_inventory(NAMES + ["receives after a new fault"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unmapped or duplicate", result.stderr)

    def test_missing_case_fails(self):
        result = self.check_inventory(NAMES[:-1])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing=", result.stderr)

    def test_duplicate_case_fails(self):
        self.assertNotEqual(self.check_inventory(NAMES + [NAMES[0]]).returncode, 0)

    def test_tcp_eof_is_not_graceful_restart(self):
        self.assertNotEqual(
            self.check_inventory(
                [name.replace("graceful backend restart", "TCP EOF") for name in NAMES]
            ).returncode,
            0,
        )

    def test_result_guard_rejects_missing_partition_case(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "results.json"
            path.write_text(
                json.dumps(
                    {
                        "testResults": [
                            {
                                "assertionResults": [
                                    {"title": NAMES[4], "status": "passed"}
                                ]
                            }
                        ]
                    }
                )
            )
            with self.assertRaisesRegex(ValueError, "Executed recovery cases"):
                module.validate_results(path, [NAMES[4], NAMES[6]])


if __name__ == "__main__":
    unittest.main()
