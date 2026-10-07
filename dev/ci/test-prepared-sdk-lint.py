#!/usr/bin/env python3.11
"""Exercise prepared SDK lint and its active Browser platform readers."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PLATFORM_FILES = (
    "sdks/browser/test/platform/suite.worker.ts",
    "sdks/browser/test/platform/attachment-lifetime.chromium.ts",
    "sdks/browser/test/platform/storage.layout.chromium.ts",
)


class PreparedLintTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.script = self.root / "crates/xmtp_sdk/dev/lint-generated"
        self.script.parent.mkdir(parents=True)
        shutil.copyfile(ROOT / "crates/xmtp_sdk/dev/lint-generated", self.script)
        for name in (
            "check-file-sizes",
            "check-fork-pin",
            "check-public-names",
            "check-test-hooks",
            "check-conformance-exports",
            "check-encoded-content-shape",
            "check-message-lifts",
            "check-wasm-init",
            "link-runtime-packages",
        ):
            self.executable(self.script.parent / name, "#!/bin/sh\nexit 0\n")
        (self.script.parent / "check-storage-path-async").write_text("pass\n")
        for tree in (
            "swift",
            "kotlin",
            "typescript-napi",
            "typescript-wasm",
            "typescript-pure",
        ):
            folder = self.root / "target/sdk-generated" / tree
            folder.mkdir(parents=True)
            (folder / "example.gen.ts").write_text("export {};\n")
            (folder / "conformance.gen.test.ts").write_text("export {};\n")
            (folder / "index.ts").write_text("export {};\n")
            (folder / "runtime").mkdir()
            (folder / "runtime/index.ts").write_text("export {};\n")
        for package in ("core", "node", "wasm"):
            folder = self.root / "target/ci-products/runtimes" / package
            folder.mkdir(parents=True)
            (folder / "package.json").write_text("{}\n")
        for name in PLATFORM_FILES:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("export {};\n")
        tools = self.root / "tools"
        tools.mkdir()
        self.executable(tools / "node", "#!/bin/sh\nexit 0\n")
        self.executable(
            tools / "cargo", "#!/bin/sh\necho forbidden Rust build >&2\nexit 99\n"
        )
        for command in ("oxlint", "oxfmt"):
            self.executable(
                self.root / "node_modules/.bin" / command, "#!/bin/sh\nexit 0\n"
            )
        # The stub records actual compiler inputs and fails on a source marker.
        # Other lint commands are unrelated to this prepared-reader contract.
        tsc = (
            f"#!{sys.executable}\n"
            "import json,os,sys\nfrom pathlib import Path\n"
            "with open(os.environ['SDK_LINT_CALLS'],'a') as log:\n"
            " log.write(json.dumps(sys.argv[1:])+'\\n')\n"
            "for name in sys.argv[1:]:\n"
            " path=Path(name)\n"
            " if path.is_file() and 'SDK_TS_FAULT' in path.read_text():\n"
            "  print('PLATFORM_SOURCE_REJECTED '+name,file=sys.stderr)\n"
            "  sys.exit(43)\n"
        )
        for name in ("node_modules/.bin/tsc", "sdks/node/node_modules/.bin/tsc"):
            self.executable(self.root / name, tsc)
        self.log = self.root / "tsc-calls.jsonl"
        self.environment = {
            **os.environ,
            "PATH": str(tools) + os.pathsep + os.environ["PATH"],
            "XMTP_SDK_PREPARED_PRODUCTS": "1",
            "XMTP_SDK_GENERATED_DIR": str(self.root / "target/sdk-generated"),
            "XMTP_SDK_RUNTIME_DIR": str(self.root / "target/ci-products/runtimes"),
            "SDK_LINT_CALLS": str(self.log),
        }

    def executable(self, path, source):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source)
        path.chmod(0o755)

    def run_lint(self):
        return subprocess.run(
            ["bash", str(self.script)],
            cwd=self.root,
            env=self.environment,
            capture_output=True,
            text=True,
        )

    def test_prepared_lint_reads_all_three_active_platform_sources(self):
        result = self.run_lint()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertTrue(
            any(all(name in call for name in PLATFORM_FILES) for call in calls)
        )
        for name in PLATFORM_FILES:
            self.assertTrue(any(name in call for call in calls), name)

    def test_prepared_lint_propagates_each_platform_source_failure(self):
        for name in PLATFORM_FILES:
            with self.subTest(source=name):
                path = self.root / name
                path.write_text("// SDK_TS_FAULT\n")
                result = self.run_lint()
                self.assertEqual(result.returncode, 43, result.stderr)
                self.assertIn("PLATFORM_SOURCE_REJECTED " + name, result.stderr)
                path.write_text("export {};\n")


if __name__ == "__main__":
    unittest.main()
