#!/usr/bin/env python3
"""Check trial rejection, environment isolation, and checkpoint behavior."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "cache_trial", Path(__file__).with_name("cache-trial.py")
)
trial = importlib.util.module_from_spec(spec)
spec.loader.exec_module(trial)


class EventChecks(unittest.TestCase):
    def test_content_mismatch_rejected_even_with_a_hit(self):
        with self.assertRaisesRegex(ValueError, "content mismatch"):
            trial.events_summary(
                [
                    {
                        "crate_name": "xmtp_sdk",
                        "result": "local_hit",
                        "verify_compare": "content: libxmtp_sdk.so",
                    }
                ],
                True,
            )

    def test_verify_requires_actual_compared_hits(self):
        for events in (
            [],
            [{"crate_name": "xmtp_sdk", "result": "miss"}],
            [{"crate_name": "xmtp_sdk", "result": "local_hit"}],
        ):
            with self.subTest(events=events), self.assertRaises(ValueError):
                trial.events_summary(events, True)

    def test_units_retain_misses_passthrough_and_path_comparisons(self):
        result = trial.events_summary(
            [
                {
                    "crate_name": "xmtp_sdk",
                    "result": "local_hit",
                    "verify_compare": "path-debug: output",
                    "elapsed_ms": 8,
                    "compile_time_ms": 900,
                },
                {"crate_name": "xmtp_sdk", "result": "miss", "compile_time_ms": 600},
                {
                    "crate_name": "native_build",
                    "result": "passthrough",
                    "passthrough_reason": "crate type",
                },
            ],
            True,
        )
        self.assertEqual(
            result["results"], {"local_hit": 1, "miss": 1, "passthrough": 1}
        )
        self.assertEqual(result["units"]["xmtp_sdk"]["compileMs"], 1500)
        self.assertEqual(
            result["units"]["native_build"]["passthroughReasons"], ["crate type"]
        )

    def test_controls_remove_inherited_wrappers_and_incremental_override(self):
        inherited = {
            "RUSTC_WRAPPER": "/action/kache",
            "RUSTC_WORKSPACE_WRAPPER": "other",
            "CARGO_BUILD_RUSTC_WRAPPER": "other",
            "CARGO_INCREMENTAL": "1",
            "RUSTFLAGS": "--cfg tracing_unstable",
            "CI": "true",
            "XMTP_TEST_LOGGING": "debug",
        }
        for arm in trial.ARMS:
            with self.subTest(arm=arm):
                env = trial.environment(arm, Path("store"), Path("runtime"), inherited)
                self.assertEqual(env["RUSTFLAGS"], inherited["RUSTFLAGS"])
                self.assertEqual(env["XMTP_TEST_LOGGING"], "debug")
                self.assertNotIn("RUSTC_WORKSPACE_WRAPPER", env)
                self.assertNotIn("CARGO_BUILD_RUSTC_WRAPPER", env)
                if arm == "current":
                    self.assertNotIn("CARGO_INCREMENTAL", env)
                    self.assertNotIn("RUSTC_WRAPPER", env)
                else:
                    self.assertEqual(env["CARGO_INCREMENTAL"], "0")
                if arm == "kache":
                    self.assertEqual(env["KACHE_KEY_ENV_VARS"], "CI,XMTP_TEST_LOGGING")
                    self.assertEqual(env["KACHE_ADAPTIVE_INCREMENTAL"], "0")
                    self.assertEqual(env["KACHE_PRESERVE_INCREMENTAL"], "0")
                    self.assertEqual(env["KACHE_REMOTE_READONLY"], "1")


class TrialLifecycle(unittest.TestCase):
    def test_fresh_targets_and_failed_checkpoint(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder) / "repo"
            root.mkdir()
            (root / "crates/xmtp_sdk/dev").mkdir(parents=True)
            (root / "crates/xmtp_sdk/dev/generate").write_text(
                '#!/bin/bash\nset -eu\nwhile [ "$#" -gt 0 ]; do\n'
                'case "$1" in --artifacts) artifacts="$2";; --out) out="$2";; esac\n'
                'shift 2\ndone\nmkdir -p "$artifacts/build/native" "$out"\n'
                'test ! -e "$artifacts/build/native/old"\n'
                'echo new > "$artifacts/build/native/old"\necho declaration > "$out/sdk.d.ts"\n'
            )
            tools = Path(folder) / "bin"
            tools.mkdir()
            for name in ("rustc", "cargo"):
                script = tools / name
                script.write_text("#!/bin/sh\necho fixture-compiler\n")
                script.chmod(0o755)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            subprocess.run(
                [
                    "git",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.test",
                    "commit",
                    "-qm",
                    "fixture",
                ],
                cwd=root,
                check=True,
            )
            output = Path(folder) / "trial"
            args = argparse.Namespace(
                output=output,
                arm="current",
                phase="cold",
                index=0,
                second_checkout=False,
            )
            env = {
                "XMTP_NIX_ENV": "yes",
                "PATH": str(tools) + os.pathsep + os.environ["PATH"],
            }
            with (
                patch.object(trial, "ROOT", root),
                patch.object(trial.sys, "platform", "linux"),
                patch.dict(os.environ, env),
            ):
                trial.init(args)
                trial.sample(args)
                args.phase = "warm"
                args.second_checkout = True
                trial.sample(args)
                state = json.loads((output / "trial.json").read_text())
                self.assertEqual(
                    [s["status"] for s in state["samples"]], ["passed", "passed"]
                )
                self.assertTrue(state["samples"][1]["secondCheckout"])
                self.assertEqual(
                    state["samples"][0]["products"], state["samples"][1]["products"]
                )
                self.assertEqual(state["qualification"].split(":")[0], "UNVERIFIED")
                args.index = 1
                args.second_checkout = False
                with patch.dict(os.environ, {"RUSTFLAGS": "--cfg changed"}):
                    with self.assertRaisesRegex(ValueError, "context changed"):
                        trial.sample(args)
                context_state = json.loads((output / "trial.json").read_text())
                self.assertEqual(context_state["samples"][-1]["status"], "failed")
                self.assertIn("context changed", context_state["samples"][-1]["error"])
                # A changed source must fail before compilation, with a saved checkpoint.
                (root / "crates/xmtp_sdk/dev/generate").write_text(
                    (root / "crates/xmtp_sdk/dev/generate").read_text()
                    + "# source change\n"
                )
                args.index = 2
                args.second_checkout = False
                with self.assertRaisesRegex(ValueError, "source changed"):
                    trial.sample(args)
                state = json.loads((output / "trial.json").read_text())
                self.assertEqual(state["samples"][-1]["status"], "failed")
                self.assertIn("source changed", state["samples"][-1]["error"])
                self.assertGreater(
                    state["samples"][-1]["sampleSecondsThroughTeardown"], 0
                )


if __name__ == "__main__":
    unittest.main()
