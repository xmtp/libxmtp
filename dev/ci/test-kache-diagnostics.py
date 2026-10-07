#!/usr/bin/env python3
"""Check safe cache counts and distinguish compilation from raw role reuse."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "kache_diagnostics", Path(__file__).with_name("kache-diagnostics.py")
)
diagnostics = importlib.util.module_from_spec(spec)
spec.loader.exec_module(diagnostics)


class CacheDiagnosticsTests(unittest.TestCase):
    def test_counts_keep_private_fields_out_and_omit_old_reuse_timing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wrapper = root / "kache"
            wrapper.write_text(
                "#!/bin/sh\nprintf 'kache 1.0.0\\nsecret-version-tail\\n'\n"
            )
            wrapper.chmod(0o755)
            events = [
                {"result": "local_hit", "elapsed_ms": 2, "argv": "secret-argv"},
                {"result": "remote_hit", "elapsed_ms": 3, "token": "secret-token"},
                {"result": "miss", "elapsed_ms": 10, "source": "secret-source"},
                {"result": "error", "error": "secret-error"},
                {"result": "passthrough"},
            ]
            (root / "events.jsonl").write_text(
                "\n".join(json.dumps(event) for event in events) + "\ninvalid-json\n7\n"
            )
            ledger = root / "artifacts.json"
            ledger.write_text(
                json.dumps(
                    {
                        "execution": [
                            {"role": "native", "action": "build"},
                            {"role": "bindgen", "action": "reuse"},
                        ],
                        "artifacts": {
                            "native": {"seconds": 4},
                            "bindgen": {"seconds": 900},
                        },
                    }
                )
            )
            result = diagnostics.collect(wrapper, root, ledger)
            self.assertEqual(
                result["resultCounts"],
                {
                    "local_hit": 1,
                    "remote_hit": 1,
                    "miss": 1,
                    "error": 1,
                    "passthrough": 1,
                    "other": 0,
                },
            )
            self.assertEqual(result["eventElapsedMs"]["miss"], 10)
            self.assertEqual(result["invalidEventLines"], 2)
            self.assertEqual(
                result["rawRoleExecution"],
                [
                    {"role": "native", "action": "build", "buildSeconds": 4},
                    {"role": "bindgen", "action": "reuse"},
                ],
            )
            self.assertEqual(result["version"], "kache 1.0.0")
            self.assertNotIn("secret", json.dumps(result))


if __name__ == "__main__":
    unittest.main()
