#!/usr/bin/env python3
"""Check both supported prebuilt runtime directory layouts."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tarfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[3]
# The artifact features that staging admits for each generated tree.
FEATURES = {
    "typescript-napi": "",
    "typescript-wasm": "",
    "typescript-pure": "pure-only",
}


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

    def read_only_browser_inputs(self, compiler_source):
        """Write browser inputs whose generated trees are read-only, as Nix outputs are."""
        generated = self.root / "generated"
        for tree in ("typescript-wasm", "typescript-pure"):
            source = generated / tree
            (source / "snippets").mkdir(parents=True)
            (source / "package.json").write_text('{"type":"module"}')
            (source / "index.ts").write_text("export const marker = 'sdk';")
            (source / "xmtp_sdk.js").write_text("export const glue = 'sdk';")
            (source / "snippets/helper.js").write_text("export const helper = 1;")
            names = ("package.json", "index.ts", "xmtp_sdk.js", "snippets/helper.js")
            (source / "sdk-contract.json").write_text(
                json.dumps(
                    {
                        "contract": "fixture",
                        "generator": "fixture",
                        "artifact": {
                            "source": "fixture",
                            "profile": "release",
                            "features": FEATURES[tree],
                        },
                        "files": {
                            name: hashlib.sha256(
                                (source / name).read_bytes()
                            ).hexdigest()
                            for name in names
                        },
                    }
                )
            )
        for name in ("core", "wasm"):
            self.runtime(self.root / "runtime", name)
        compiler = self.root / "compiler.mjs"
        compiler.write_text(compiler_source)
        paths = sorted(generated.rglob("*"), reverse=True) + [generated]
        for path in paths:
            path.chmod(0o555 if path.is_dir() else 0o444)
        output = self.root / "packages"
        env = dict(
            os.environ,
            XMTP_SDK_GENERATED_DIR=str(generated),
            XMTP_SDK_PACKAGES_DIR=str(output),
            XMTP_SDK_RUNTIME_DIR=str(self.root / "runtime"),
            XMTP_SDK_TSDOWN_CLI=str(compiler),
        )
        command = [
            "node",
            str(ROOT / "crates/xmtp_sdk/dev/stage-package.mjs"),
            "browser",
        ]
        return (
            paths,
            output,
            lambda: subprocess.run(command, env=env, capture_output=True, text=True),
        )

    def test_read_only_generated_tree_stages_without_writes(self):
        paths, output, stage = self.read_only_browser_inputs(
            "import {writeFileSync} from 'node:fs';\n"
            "const {default: config} = await import(process.argv[3]);\n"
            "writeFileSync(config.outDir + '/index.js', \"export const marker = 'sdk';\");\n"
            "writeFileSync(config.outDir + '/index.d.ts', 'export declare const marker: string;');\n"
        )
        before = {path: path.read_bytes() for path in paths if path.is_file()}
        result = stage()
        self.assertEqual(result.returncode, 0, result.stderr)
        product = output / "browser"
        self.assertEqual(
            json.loads((product / "package.json").read_text())["version"],
            tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"][
                "version"
            ],
        )
        for tree in ("typescript-wasm", "typescript-pure"):
            self.assertTrue((product / tree / "index.js").is_file())
            self.assertTrue((product / tree / "snippets/helper.js").is_file())
        self.assertEqual([path.name for path in output.iterdir()], ["browser"])
        self.assertEqual(
            {path: path.read_bytes() for path in paths if path.is_file()}, before
        )
        for path in paths:
            self.assertFalse(path.stat().st_mode & 0o222, path)

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
                    "artifact": {
                        "source": "fixture",
                        "profile": "release",
                        "features": "",
                    },
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
                        *(["--public"] if layout == "prebuilt" else []),
                    ],
                    env=env,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                manifest = json.loads((out / "node/package.json").read_text())
                self.assertEqual(
                    manifest["version"],
                    tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"][
                        "package"
                    ]["version"],
                )
                if layout == "prebuilt":
                    source = json.loads((ROOT / "sdks/node/package.json").read_text())
                    self.assertEqual(manifest["name"], source["name"])
                    self.assertEqual(manifest["version"], source["version"])
                    self.assertNotIn("private", manifest)
                    self.assertEqual(
                        manifest["exports"]["."],
                        {
                            "types": "./entry.d.ts",
                            "import": "./entry.js",
                        },
                    )
                else:
                    self.assertTrue(manifest["private"])
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
                if layout == "prebuilt":
                    for version in (None, "8.0.1-rc.1"):
                        if version:
                            subprocess.run(
                                [
                                    str(
                                        ROOT / "dev/release-tools/node_modules/.bin/tsx"
                                    ),
                                    str(ROOT / "dev/release-tools/src/cli.ts"),
                                    "stamp-package",
                                    "--dir",
                                    str(out / "node"),
                                    "--version",
                                    version,
                                ],
                                # tsx reads the CLI's path aliases from here.
                                cwd=ROOT / "dev/release-tools",
                                check=True,
                                capture_output=True,
                            )
                        packed_dir = consumer / (version or "staged")
                        packed_dir.mkdir()
                        subprocess.run(
                            [
                                "pnpm",
                                "--config.node-linker=hoisted",
                                "--config.ignore-scripts=true",
                                "pack",
                                "--pack-destination",
                                str(packed_dir),
                                "--json",
                            ],
                            cwd=out / "node",
                            check=True,
                            capture_output=True,
                        )
                        (archive_path,) = packed_dir.glob("*.tgz")
                        with tarfile.open(archive_path) as archive:
                            receipt = json.load(
                                archive.extractfile("package/sdk-contract.json")
                            )
                            for filename, checksum in receipt["assets"].items():
                                packed_bytes = archive.extractfile(
                                    "package/" + filename
                                ).read()
                                self.assertEqual(
                                    hashlib.sha256(packed_bytes).hexdigest(),
                                    checksum,
                                    f"pnpm packed asset mismatch after {version or 'stage'}: {filename}",
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
                installed = consumer / "node_modules" / manifest["name"]
                check = [
                    "node",
                    "--input-type=module",
                    "-e",
                    "import './entry.js';",
                ]
                subprocess.run(check, cwd=installed, check=True, capture_output=True)
                # The receipt describes the build. Import must not need it.
                (installed / "sdk-contract.json").unlink()
                subprocess.run(check, cwd=installed, check=True, capture_output=True)
                self.assertFalse((installed / "sdk-contract-check.js").exists())
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
