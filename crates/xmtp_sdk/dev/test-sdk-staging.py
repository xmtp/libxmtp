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
        self.root = Path(self.temporary.name).resolve()

    def tearDown(self):
        self.temporary.cleanup()

    def runtime(self, root, name):
        folder = root / name
        folder.mkdir(parents=True)
        (folder / "package.json").write_text(
            json.dumps(
                {
                    "name": "@ubjs/" + name,
                    "version": "0.0.0",
                    "module": "index.js",
                    "files": ["index.js", "other.js", "binding.node"],
                }
            )
        )
        (folder / "index.js").write_text("export const marker = 'runtime';")
        (folder / "other.js").write_text("export const other = 'runtime';")
        (folder / "package-lock.json").write_text('{"lockfileVersion":3}')
        (folder / "not-shipped.txt").write_text("excluded runtime build input")
        if name == "node":
            (folder / "binding.node").write_bytes(b"fixture native asset")

    def test_failed_rebuild_preserves_prior_product_and_success_replaces_it(self):
        generated = self.root / "generated/typescript-napi"
        generated.mkdir(parents=True)
        (generated / "package.json").write_text('{"type":"module"}')
        (generated / "index.ts").write_text("export const marker = 'sdk';")
        (generated / "sdk-contract.json").write_text(
            json.dumps(
                {
                    "contract": "fixture",
                    "generator": "fixture",
                    "files": {
                        name: hashlib.sha256(
                            (generated / name).read_bytes()
                        ).hexdigest()
                        for name in ("package.json", "index.ts")
                    },
                }
            )
        )
        for name in ("core", "node"):
            self.runtime(self.root / "runtime", name)
        compiler = self.root / "compiler.mjs"
        compiler_source = (
            "import {writeFileSync} from 'node:fs'; const {default:c}=await import(process.argv[3]);"
            "writeFileSync(c.outDir+'/index.js',\"export const marker='old';\");"
            "writeFileSync(c.outDir+'/index.d.ts','export declare const marker:string;');"
        )
        compiler.write_text(compiler_source)
        output = self.root / "packages"
        env = dict(
            os.environ,
            XMTP_SDK_GENERATED_DIR=str(generated.parent),
            XMTP_SDK_PACKAGES_DIR=str(output),
            XMTP_SDK_RUNTIME_DIR=str(self.root / "runtime"),
            XMTP_SDK_TSDOWN_CLI=str(compiler),
        )
        command = ["node", str(ROOT / "crates/xmtp_sdk/dev/stage-package.mjs"), "node"]

        def stage(environment):
            return subprocess.run(
                command, env=environment, capture_output=True, text=True
            )

        def snapshot():
            return {
                str(path.relative_to(output / "node")): hashlib.sha256(
                    path.read_bytes()
                ).hexdigest()
                for path in (output / "node").rglob("*")
                if path.is_file()
            }

        good = stage(env)
        self.assertEqual(good.returncode, 0, good.stderr)
        before = snapshot()
        rename_failure = self.root / "rename-failure.mjs"
        rename_failure.write_text(
            "import fs from 'node:fs'; import {syncBuiltinESMExports} from 'node:module';"
            "const rename=fs.renameSync; fs.renameSync=(from,to)=>{"
            "if(String(from).includes('/.sdk-stage-') && String(from).endsWith('/node')) "
            "throw new Error('stage replacement failure'); return rename(from,to);};"
            "syncBuiltinESMExports();"
        )
        failed = subprocess.run(
            ["node", "--import", str(rename_failure), *command[1:]],
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn("stage replacement failure", failed.stderr)
        self.assertEqual(snapshot(), before)
        self.assertEqual([path.name for path in output.iterdir()], ["node"])
        compiler.write_text("throw new Error('stage compiler failure');")
        failed = stage(env)
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn("stage compiler failure", failed.stderr)
        self.assertEqual(snapshot(), before)
        compiler.write_text(compiler_source)
        failed_npm = self.root / "failed-npm.mjs"
        failed_npm.write_text("console.log('invalid npm metadata');")
        failed_env = dict(env, XMTP_SDK_NPM_CLI=str(failed_npm))
        failed = stage(failed_env)
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn("invalid npm metadata", failed.stderr)
        self.assertEqual(snapshot(), before)
        fresh = self.root / "fresh-packages"
        failed = stage(dict(failed_env, XMTP_SDK_PACKAGES_DIR=str(fresh)))
        self.assertNotEqual(failed.returncode, 0)
        self.assertFalse((fresh / "node").exists())
        self.assertEqual(list(fresh.iterdir()), [])
        compiler.write_text(compiler_source.replace("marker='old'", "marker='new'"))
        good = stage(env)
        self.assertEqual(good.returncode, 0, good.stderr)
        self.assertNotEqual(snapshot(), before)
        self.assertIn("marker='new'", (output / "node/index.js").read_text())
        check = subprocess.run(
            ["node", "--input-type=module", "-e", "import './sdk-contract-check.js';"],
            cwd=output / "node",
            capture_output=True,
            text=True,
        )
        self.assertEqual(check.returncode, 0, check.stderr)
        self.assertEqual([path.name for path in output.iterdir()], ["node"])

    def test_flat_and_prebuilt_runtime_products_stage(self):
        generated = self.root / "generated/typescript-napi"
        generated.mkdir(parents=True)
        (generated / "package.json").write_text('{"type":"module"}')
        (generated / "index.ts").write_text("export const marker = 'sdk';")
        (generated / "sdk-contract.json").write_text(
            json.dumps(
                {
                    "contract": "fixture",
                    "generator": "fixture",
                    "files": {
                        name: hashlib.sha256(
                            (generated / name).read_bytes()
                        ).hexdigest()
                        for name in ("package.json", "index.ts")
                    },
                }
            )
        )
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
                metadata = json.loads((out / "node/sdk-contract.json").read_text())
                self.assertNotIn(
                    "node_modules/@ubjs/core/package-lock.json", metadata["assets"]
                )
                self.assertNotIn(
                    "node_modules/@ubjs/core/not-shipped.txt", metadata["assets"]
                )
                consumer = self.root / (layout + "-consumer")
                consumer.mkdir()
                (consumer / "package.json").write_text(
                    '{"private":true,"type":"module"}'
                )
                packed = json.loads(
                    subprocess.check_output(
                        [
                            "npm",
                            "pack",
                            str(out / "node"),
                            "--pack-destination",
                            str(consumer),
                            "--json",
                            "--ignore-scripts",
                        ],
                        cwd=out / "node",
                        text=True,
                    )
                )[0]
                self.assertEqual(set(packed["bundled"]), {"@ubjs/core", "@ubjs/node"})
                subprocess.run(
                    [
                        "npm",
                        "install",
                        "--ignore-scripts",
                        "--no-audit",
                        "--no-fund",
                        "--package-lock=false",
                        str(consumer / packed["filename"]),
                    ],
                    cwd=consumer,
                    check=True,
                    capture_output=True,
                )
                installed = consumer / "node_modules/xmtp-sdk"
                check = [
                    "node",
                    "--input-type=module",
                    "-e",
                    "import './sdk-contract-check.js';",
                ]
                subprocess.run(check, cwd=installed, check=True, capture_output=True)
                for filename in (
                    "node_modules/@ubjs/core/index.js",
                    "node_modules/@ubjs/node/binding.node",
                ):
                    runtime = installed / filename
                    original = runtime.read_bytes()
                    runtime.write_bytes(original + b"changed")
                    failed = subprocess.run(
                        check, cwd=installed, capture_output=True, text=True
                    )
                    self.assertNotEqual(failed.returncode, 0)
                    self.assertIn("SDK asset mismatch: " + filename, failed.stderr)
                    runtime.write_bytes(original)
                subprocess.run(check, cwd=installed, check=True, capture_output=True)
                for name in ("core", "node"):
                    entry = products / name / "index.js"
                    original_entry = entry.read_bytes()
                    entry.unlink()
                    omitted_entry = subprocess.run(
                        [
                            "node",
                            str(ROOT / "crates/xmtp_sdk/dev/stage-package.mjs"),
                            "node",
                        ],
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    entry.write_bytes(original_entry)
                    self.assertNotEqual(omitted_entry.returncode, 0)
                    self.assertIn(
                        "omits required runtime entry: node_modules/@ubjs/"
                        + name
                        + "/index.js",
                        omitted_entry.stderr,
                    )
                for name, allowed, message in (
                    (
                        "core",
                        ["package.json"],
                        "omits required runtime entry: node_modules/@ubjs/core/index.js",
                    ),
                    ("node", ["index.js"], "omits native runtime binary"),
                ):
                    runtime_manifest = products / name / "package.json"
                    original_manifest = runtime_manifest.read_text()
                    changed_manifest = json.loads(original_manifest)
                    changed_manifest["files"] = allowed
                    if name == "core":
                        changed_manifest.pop("module", None)
                        changed_manifest.pop("main", None)
                    runtime_manifest.write_text(json.dumps(changed_manifest))
                    omitted = subprocess.run(
                        [
                            "node",
                            str(ROOT / "crates/xmtp_sdk/dev/stage-package.mjs"),
                            "node",
                        ],
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    runtime_manifest.write_text(original_manifest)
                    self.assertNotEqual(omitted.returncode, 0)
                    self.assertIn(message, omitted.stderr)


if __name__ == "__main__":
    unittest.main()
