#!/usr/bin/env python3
"""Keep native release checks active when Python optimization is enabled."""

import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest

GATE = Path(__file__).with_name("check-native-nix.py")
RUN = """
import json, runpy, sys
from unittest.mock import patch
products = json.loads(sys.argv[2])
with patch('subprocess.check_output', return_value=json.dumps(products).encode()):
    runpy.run_path(sys.argv[1], run_name='__main__')
"""


def products():
    inputs = {
        "jobs": 2,
        "vendor": "0",
        "static": "1",
        "macos": "11.0",
        "command": 'export IPHONEOS_DEPLOYMENT_TARGET="14"',
    }
    return {
        name: {"main": inputs.copy(), "deps": inputs.copy()}
        for name in (
            "xmtp-sdk-libs", "xmtp-sdk-bindgen", "xmtp-sdk-wasm",
            "xmtp-sdk-pure-wasm", "xmtp-sdk-ios-device", "xmtp-sdk-ios-simulator",
        )
    }


class NativeGate(unittest.TestCase):
    def run_gate(self, value, optimized):
        return subprocess.run(
            [sys.executable, *(["-O"] if optimized else []), "-c", RUN,
             str(GATE), json.dumps(value)],
            capture_output=True, text=True,
        )

    def test_valid_inputs_pass_with_and_without_optimization(self):
        for optimized in (False, True):
            with self.subTest(optimized=optimized):
                result = self.run_gate(products(), optimized)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(json.loads(result.stdout), products())

    def test_each_release_requirement_rejects_in_both_build_phases(self):
        cases = (
            ("xmtp-sdk-wasm", "jobs", 3, "Cargo job limit"),
            ("xmtp-sdk-libs", "vendor", "1", "vendored OpenSSL"),
            ("xmtp-sdk-libs", "static", "0", "static OpenSSL"),
            ("xmtp-sdk-libs", "macos", "14.0", "macOS floor"),
            ("xmtp-sdk-ios-simulator", "command", "", "iOS floor"),
        )
        for name, field, value, reason in cases:
            for phase in ("main", "deps"):
                for optimized in (False, True):
                    with self.subTest(name=name, phase=phase, optimized=optimized):
                        changed = copy.deepcopy(products())
                        changed[name][phase][field] = value
                        result = self.run_gate(changed, optimized)
                        self.assertNotEqual(result.returncode, 0, result.stdout)
                        self.assertIn(reason, result.stderr)


if __name__ == "__main__":
    unittest.main()
