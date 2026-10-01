#!/usr/bin/env python3
"""Check both supported prebuilt runtime directory layouts."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]


class StagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def runtime(self, root, name):
        folder = root / name
        folder.mkdir(parents=True)
        (folder / "package.json").write_text(
            json.dumps(
                {"name": "@ubjs/" + name, "version": "0.0.0", "module": "index.js"}
            )
        )
        (folder / "index.js").write_text("export const marker = 'runtime';")
        if name == "node":
            (folder / "binding.node").write_bytes(b"fixture native asset")

    def test_flat_and_prebuilt_runtime_products_stage(self):
        generated = self.root / "generated/typescript-napi"
        generated.mkdir(parents=True)
        (generated / "sdk-contract.json").write_text(
            json.dumps({"contract": "fixture", "generator": "fixture", "files": {}})
        )
        (generated / "package.json").write_text('{"type":"module"}')
        (generated / "index.ts").write_text("export const marker = 'sdk';")
        compiler = self.root / "compiler.mjs"
        compiler.write_text(
            "import {writeFileSync} from 'node:fs';\n"
            "const {default: config} = await import(process.argv[3]);\n"
            "writeFileSync(config.outDir + '/index.js', \"export const marker = 'sdk';\");\n"
            "writeFileSync(config.outDir + '/index.d.ts', 'export declare const marker: string;');\n"
        )
        expected = hashlib.sha256(b"fixture native asset").hexdigest()
        for layout in ("flat", "prebuilt"):
            with self.subTest(layout=layout):
                runtimes = self.root / layout
                products = (
                    runtimes
                    if layout == "flat"
                    else runtimes / "lib/node_modules/@ubjs"
                )
                for name in ("core", "node"):
                    self.runtime(products, name)
                out = self.root / (layout + "-packages")
                env = dict(
                    os.environ,
                    XMTP_SDK_GENERATED_DIR=str(generated.parent),
                    XMTP_SDK_PACKAGES_DIR=str(out),
                    XMTP_SDK_RUNTIME_DIR=str(runtimes),
                    XMTP_SDK_TSDOWN_CLI=str(compiler),
                )
                result = subprocess.run(
                    [
                        "node",
                        str(ROOT / "crates/xmtp_sdk/dev/stage-package.mjs"),
                        "node",
                    ],
                    env=env,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                asset = out / "node/node_modules/@ubjs/node/binding.node"
                self.assertEqual(
                    hashlib.sha256(asset.read_bytes()).hexdigest(), expected
                )
                self.assertEqual(
                    json.loads((out / "node/package.json").read_text())[
                        "bundledDependencies"
                    ],
                    ["@ubjs/core", "@ubjs/node"],
                )


if __name__ == "__main__":
    unittest.main()
