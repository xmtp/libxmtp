#!/usr/bin/env python3
"""Check trial rejection, environment isolation, and checkpoint behavior."""

import argparse
import importlib.util
import json
import os
import shutil
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

    def test_summary_stays_small_with_full_key_and_product_records(self):
        state = {
            "arm": "kache",
            "scenario": "stable",
            "samples": [
                {
                    "phase": "warm",
                    "index": 0,
                    "status": "passed",
                    "generationSeconds": 2,
                    "products": {f"product-{i}": "f" * 64 for i in range(20000)},
                    "cache": {
                        "results": {"local_hit": 99, "miss": 1},
                        "units": {
                            f"crate-{i}": {"wrapperMs": i, "results": {"local_hit": 1}}
                            for i in range(20000)
                        },
                    },
                }
            ],
        }
        summary = trial.compact_summary(state)
        self.assertLess(len(summary.encode()), 8192)
        self.assertIn("local_hit=99", summary)
        self.assertIn("Qualification: UNVERIFIED", summary)
        self.assertEqual(summary.count("wrapper "), 8)

    def test_key_diagnostics_retain_fields_and_dependency_hashes(self):
        event = {
            "crate_name": "cfg_if",
            "result": "miss",
            "cache_key": "cold-key",
            "key_fields": {"args": "args-hash", "sources": "source-hash"},
            "key_externs": {"dep": "dep-hash"},
            "root": "/checkout/target",
            "compiler_runs": 1,
        }
        rows = trial.key_diagnostics([event, {"event": "heartbeat"}])
        self.assertEqual(len(rows), 1)
        self.assertEqual(
            rows[0].get("key_fields"), {"args": "args-hash", "sources": "source-hash"}
        )
        self.assertEqual(rows[0].get("key_externs"), {"dep": "dep-hash"})
        self.assertEqual(rows[0]["cache_key"], "cold-key")


class TrialLifecycle(unittest.TestCase):
    def test_fresh_targets_and_failed_checkpoint(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve() / "repo"
            root.mkdir()
            (root / "crates/xmtp_sdk/dev").mkdir(parents=True)
            (root / ".cargo").mkdir()
            (root / ".cargo/config.toml").write_text(
                '[target."cfg(all())"]\nrustflags = ["--cfg", "tracing_unstable"]\n'
            )
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
                scenario="stable",
                diagnostics=True,
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
                trial.sample(args)
                state = json.loads((output / "trial.json").read_text())
                self.assertEqual(
                    [s["status"] for s in state["samples"]], ["passed", "passed"]
                )
                self.assertFalse(state["samples"][1]["secondCheckout"])
                self.assertEqual(
                    state["samples"][0]["checkout"], state["samples"][1]["checkout"]
                )
                self.assertEqual(
                    state["samples"][0]["artifactRoot"],
                    state["samples"][1]["artifactRoot"],
                )
                self.assertFalse(
                    Path(state["samples"][0]["checkout"]).is_relative_to(root)
                )
                self.assertFalse(state["samples"][1]["timedArm"])
                self.assertEqual(
                    state["samples"][0]["products"], state["samples"][1]["products"]
                )
                self.assertEqual(state["qualification"].split(":")[0], "UNVERIFIED")
                for row in state["samples"]:
                    local = [
                        item
                        for item in row["cargoConfigAncestry"]
                        if item["scope"] == "workspace"
                    ]
                    self.assertEqual(len(local), 1)
                    self.assertEqual(
                        Path(local[0]["path"]).parent.parent, Path(row["checkout"])
                    )
                    self.assertNotIn(
                        str(root / ".cargo/config.toml"),
                        [item["path"] for item in row["cargoConfigAncestry"]],
                    )
                # The optional cross-checkout case also keeps source/config identity.
                state["scenario"] = "cross-checkout"
                trial.save(output / "trial.json", state)
                args.index = 1
                trial.sample(args)
                state = json.loads((output / "trial.json").read_text())
                self.assertTrue(state["samples"][-1]["secondCheckout"])
                self.assertNotEqual(
                    state["samples"][0]["checkout"], state["samples"][-1]["checkout"]
                )
                self.assertEqual(
                    state["samples"][0]["products"], state["samples"][-1]["products"]
                )
                # Cleanup must not follow an output parent into foreign data.
                state["scenario"] = "stable"
                trial.save(output / "trial.json", state)
                checkout = Path(state["samples"][0]["checkout"])
                shutil.rmtree(checkout / "target")
                foreign = Path(folder) / "foreign"
                (foreign / "sdk-artifacts").mkdir(parents=True)
                sentinel = foreign / "sdk-artifacts/keep"
                sentinel.write_text("foreign data")
                (checkout / "target").symlink_to(foreign, target_is_directory=True)
                args.index = 2
                with self.assertRaisesRegex(ValueError, "output escaped"):
                    trial.sample(args)
                self.assertEqual(sentinel.read_text(), "foreign data")
                (checkout / "target").unlink()
                args.index = 3
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
                args.index = 4
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
