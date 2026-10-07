#!/usr/bin/env python3
"""Protect recovery case ownership at the real command entry point."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import yaml

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
    def test_recovery_has_only_a_manual_owner(self):
        normal = yaml.safe_load(
            (ROOT / ".github/workflows/test-node-sdk.yml").read_text()
        )
        self.assertNotIn("recovery", normal["jobs"])
        manual = yaml.safe_load(
            (ROOT / ".github/workflows/manual-sdk-recovery.yml").read_text()
        )
        self.assertEqual(set(manual.get("on", manual.get(True))), {"workflow_dispatch"})
        jobs = manual["jobs"]
        self.assertEqual(set(jobs), {"sdk-node", "backend-products", "recovery"})
        self.assertEqual(jobs["sdk-node"]["with"]["kind"], "node")
        self.assertEqual(jobs["backend-products"]["with"]["kind"], "backend")
        recovery = jobs["recovery"]
        self.assertEqual(recovery["strategy"]["matrix"]["group"], list(range(1, 9)))
        self.assertTrue(recovery["strategy"]["fail-fast"])
        self.assertEqual(set(recovery["needs"]), {"sdk-node", "backend-products"})
        steps = recovery["steps"]
        run = next(
            step
            for step in steps
            if step.get("name") == "Run isolated recovery partition"
        )
        self.assertEqual(
            run["run"],
            "dev/nix-shell 'just js test-node-sdk-recovery-prepared ${{ matrix.group }}'",
        )
        self.assertEqual(
            run["env"]["XMTP_RECOVERY_BACKEND_BINARY"],
            "${{ steps.backend-product.outputs.binary-path }}",
        )
        cleanup = next(
            step
            for step in steps
            if step.get("name") == "Stop the isolated backend stack"
        )
        self.assertEqual(cleanup["if"], "always()")
        self.assertEqual(cleanup["run"], "dev/nix-shell 'just backend down'")
        self.assertEqual(recovery["timeout-minutes"], 60)

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
        self.assertEqual(
            groups, {str(number + 1): [name] for number, name in enumerate(NAMES)}
        )

    def test_each_group_has_exactly_one_case(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "cases.json"
            groups = {str(number + 1): [name] for number, name in enumerate(NAMES)}
            groups["1"] = []
            groups["2"] = [NAMES[0], NAMES[1]]
            path.write_text(json.dumps(groups))
            with self.assertRaisesRegex(ValueError, "exactly one case"):
                module.case_map(path)

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
