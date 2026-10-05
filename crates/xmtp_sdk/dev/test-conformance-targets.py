#!/usr/bin/env python3
"""Check target selection with real generated callback converters."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument(
    "generated", type=Path, nargs="?", default=ROOT / "target/sdk-generated"
)
parser.add_argument(
    "--helper", type=Path, default=ROOT / "crates/xmtp_sdk/dev/conformance-generate"
)
args = parser.parse_args()
GENERATED = args.generated.resolve()
HELPER = args.helper.resolve()
BINDINGS = {
    "swift": "swift/xmtp_sdk.swift",
    "kotlin": "kotlin/uniffi/xmtp_sdk/xmtp_sdk.kt",
    "node": "typescript-napi/xmtp_sdk.ts",
}
RENDERER = """import argparse
import os
from pathlib import Path
import shutil
p = argparse.ArgumentParser()
p.add_argument("operation")
p.add_argument("--targets", default="swift,kotlin,node,browser")
p.add_argument("--features")
p.add_argument("--artifacts")
p.add_argument("--out", default="target/sdk-conformance")
a = p.parse_args()
if a.operation == "render":
    out = Path(a.out)
    source = Path(os.environ["SDK_TEST_GENERATED"])
    bindings = {"swift":"swift/xmtp_sdk.swift", "kotlin":"kotlin/uniffi/xmtp_sdk/xmtp_sdk.kt", "node":"typescript-napi/xmtp_sdk.ts"}
    for target in a.targets.split(","):
        names = [bindings[target]] if target != "browser" else ["typescript-wasm/index.ts", "typescript-pure/index.ts"]
        for name in names:
            destination = out / name
            tree = out / Path(name).parts[0]
            if tree.exists():
                shutil.rmtree(tree)
            destination.parent.mkdir(parents=True, exist_ok=True)
            if not os.environ.get("SDK_TEST_OMIT_BINDING"):
                shutil.copy2(source / name, destination)
"""


class TargetTests(unittest.TestCase):
    def check_targets(self, selected, missing=False, earlier=()):
        targets = selected or "swift,kotlin,node,browser"
        with tempfile.TemporaryDirectory(prefix="sdk-conformance-target-") as temporary:
            root = Path(temporary)
            dev = root / "crates/xmtp_sdk/dev"
            conformance = root / "crates/xmtp_sdk/conformance"
            dev.mkdir(parents=True)
            conformance.mkdir(parents=True)
            shutil.copy2(HELPER, dev / "conformance-generate")
            shutil.copy2(
                ROOT / "crates/xmtp_sdk/conformance/inject_callback_counts.py",
                conformance,
            )
            (dev / "sdk-artifacts.py").write_text(RENDERER)

            def generate(chosen, omit=False):
                command = ["bash", str(dev / "conformance-generate")]
                if chosen:
                    command += ["--targets", chosen]
                return subprocess.run(
                    command,
                    cwd=root,
                    env=os.environ
                    | {
                        "SDK_TEST_GENERATED": str(GENERATED),
                        "SDK_TEST_OMIT_BINDING": "1" if omit else "",
                    },
                    text=True,
                    capture_output=True,
                )

            # CI runs several targets in sequence against one output directory.
            for previous in earlier:
                result = generate(previous)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = generate(selected, missing)
            if missing:
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("No such file or directory", result.stderr)
                self.assertIn(BINDINGS["swift"], result.stderr)
                return
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            expected = set(targets.split(","))
            for target, binding in BINDINGS.items():
                path = root / "target/sdk-conformance" / binding
                self.assertEqual(path.exists(), target in expected)
                if path.exists():
                    text = path.read_text()
                    self.assertEqual(
                        text.count("sdkConformanceCallbackHandleCounts"), 1
                    )
                    counters = text.split("sdkConformanceCallbackHandleCounts", 1)[1]
                    self.assertIn("foreignFutures", counters)
                    for family in (
                        "Signer",
                        "PreAuthenticate",
                        "CredentialSource",
                        "EventListener",
                        "LogSink",
                    ):
                        self.assertIn("FfiConverterType" + family, counters)
            for language in ("typescript-wasm", "typescript-pure"):
                self.assertEqual(
                    (root / "target/sdk-conformance" / language).exists(),
                    "browser" in expected,
                )

    def test_missing_requested_binding(self):
        self.check_targets("swift", missing=True)

    def test_sequential_targets(self):
        self.check_targets("kotlin", earlier=["node"])


for name, targets in [
    ("swift", "swift"),
    ("kotlin", "kotlin"),
    ("node", "node"),
    ("browser", "browser"),
    ("mixed", "swift,node"),
    ("default", None),
]:

    def case(self, selected=targets):
        self.check_targets(selected)

    setattr(TargetTests, "test_" + name, case)

unittest.main(argv=["test-conformance-targets"], verbosity=2)
