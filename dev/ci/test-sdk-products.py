#!/usr/bin/env python3
"""Exercise portable SDK transport through the public staging and CLI paths."""

import hashlib
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = "dev/ci/sdk-products.py"
STAGE = "crates/xmtp_sdk/dev/stage-package.mjs"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class ProductTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.repo = self.base / "producer"
        self.repo.mkdir()
        # The fixtures substitute only Rust compilation and tsdown. The real
        # stager creates package/runtime receipts and the real CLI transports them.
        names = (
            SCRIPT,
            "crates/xmtp_sdk/dev/sdk-artifacts.py",
            STAGE,
            "crates/xmtp_sdk/dev/check-generated-assets.mjs",
            "crates/xmtp_sdk/dev/node-platforms.mjs",
            "Cargo.toml",
            "Cargo.lock",
            "flake.lock",
            "rust-toolchain.toml",
            "pnpm-lock.yaml",
            "package.json",
            "sdks/node/package.json",
            "sdks/browser/package.json",
            "dev/js/sdk-package",
            "dev/js/.setup",
            "crates/xmtp_sdk/dev/link-runtime-packages",
        )
        for name in names:
            destination = self.repo / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, destination)
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        subprocess.run(["git", "-C", str(self.repo), "add", "."], check=True)
        subprocess.run(
            [
                "git",
                "-C",
                str(self.repo),
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.com",
                "commit",
                "-qm",
                "fixture",
            ],
            check=True,
        )
        self.env = {
            name: value
            for name, value in os.environ.items()
            if not name.startswith(("GITHUB_", "XMTP_SDK_"))
        }
        self.compiler = self.base / "tsdown.mjs"
        self.compiler.write_text(
            "import {writeFileSync,mkdirSync} from 'node:fs'; import {relative,dirname,join} from 'node:path';"
            "const {default: c}=await import(process.argv[3]);"
            "for(const entry of c.entry){ const path=join(c.outDir,relative(c.root,entry).replace(/\\.ts$/,'.js'));"
            "mkdirSync(dirname(path),{recursive:true}); writeFileSync(path,\"export const marker='sdk';\");"
            "writeFileSync(path.replace(/\\.js$/,'.d.ts'),'export declare const marker:string;');}"
        )

    def command(self, root, *args, env=None):
        return subprocess.run(
            ["python3", "-B", str(root / SCRIPT), *args],
            cwd=root,
            env=env or self.env,
            capture_output=True,
            text=True,
        )

    def stage(self, target):
        generated = self.repo / "target/sdk-generated"
        # Use the common receipt producer's source hashes, then let real
        # stage-package.mjs validate bytes and produce its own final receipt.
        hashes = subprocess.check_output(
            [
                "python3",
                "-B",
                "-c",
                "import importlib.util,json; s=importlib.util.spec_from_file_location('a','crates/xmtp_sdk/dev/sdk-artifacts.py'); a=importlib.util.module_from_spec(s); s.loader.exec_module(a); print(json.dumps([a.source_hash(),a.source_hash(True)]))",
            ],
            cwd=self.repo,
            env=self.env,
            text=True,
        )
        source_hash, generator = json.loads(hashes)
        trees = (
            ("typescript-napi", "swift", "kotlin")
            if target == "node"
            else ("typescript-wasm", "typescript-pure")
        )
        for tree in trees:
            folder = generated / tree
            folder.mkdir(parents=True)
            (folder / "index.ts").write_text("export const marker = 'sdk';\n")
            (folder / "package.json").write_text('{"type":"module"}')
            if target == "node":
                library = folder / "libxmtp_sdk.so"
                library.write_bytes(b"fixture SDK shared library")
            else:
                library = folder / "xmtp_sdk_bg.wasm"
                library.write_bytes(b"\0asm\x01\0\0\0")
                (folder / "xmtp_sdk_bg.js").write_text("export const wasm='fixture';")
                if tree == "typescript-wasm":
                    (folder / "worker.ts").write_text("export const worker='fixture';")
            receipt = {
                "source": source_hash,
                "generator": generator,
                "profile": "debug",
                "features": "pure-only" if tree == "typescript-pure" else "",
                "target": "",
                "buildContextHash": "b" * 64,
                "compilerIdentity": "rustc fixture\nhost: "
                + (
                    "aarch64-apple-darwin"
                    if platform.system() == "Darwin"
                    else "x86_64-unknown-linux-gnu"
                )
                + "\n",
                "instrumentation": "none",
                "profileOverrides": {},
                "compilerFlags": {"RUSTFLAGS": "", "CARGO_ENCODED_RUSTFLAGS": ""},
                "files": {str(library): digest(library)},
            }
            (folder / "sdk-contract.json").write_text(
                json.dumps(
                    {
                        "contract": "matched-fixture",
                        "generator": generator,
                        "artifact": receipt,
                        "files": {p.name: digest(p) for p in folder.iterdir()},
                    }
                )
            )
        runtime = self.base / (target + "-runtime")
        for name in ("core", "node" if target == "node" else "wasm"):
            folder = runtime / name
            folder.mkdir(parents=True)
            (folder / "package.json").write_text(
                json.dumps(
                    {
                        "name": "@ubjs/" + name,
                        "version": "0.0.0",
                        "module": "index.js",
                        "files": ["index.js", "binding.node"],
                    }
                )
            )
            (folder / "index.js").write_text("export const runtime='fixture';")
            if name == "node":
                (folder / "binding.node").write_bytes(b"pinned fixture native runtime")
        env = {
            **self.env,
            "XMTP_SDK_GENERATED_DIR": str(generated),
            "XMTP_SDK_RUNTIME_DIR": str(runtime),
            "XMTP_SDK_TSDOWN_CLI": str(self.compiler),
        }
        staged = subprocess.run(
            ["node", str(self.repo / STAGE), target, "--public"],
            cwd=self.repo,
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(staged.returncode, 0, staged.stderr)
        archive = self.base / (target + ".tar")
        exported = self.command(
            self.repo, "export", "--target", target, "--output", str(archive)
        )
        self.assertEqual(exported.returncode, 0, exported.stderr)
        return archive

    def consumer(self):
        repo = self.base / "consumer"
        shutil.copytree(self.repo, repo, ignore=shutil.ignore_patterns("target"))
        return repo

    def mutate(self, original, transform):
        payload = self.base / "mutated"
        if payload.exists():
            shutil.rmtree(payload)
        payload.mkdir()
        with tarfile.open(original) as tar:
            tar.extractall(payload, filter="data")
        manifest = json.loads((payload / "manifest.json").read_text())
        transform(payload, manifest)
        (payload / "manifest.json").write_text(json.dumps(manifest))
        archive = self.base / "changed.tar"
        with tarfile.open(archive, "w") as tar:
            for path in sorted(payload.rglob("*")):
                if path.is_file():
                    tar.add(path, arcname=path.relative_to(payload), recursive=False)
        return archive

    def assert_rejected(self, repo, archive, target, reason, env=None):
        result = self.command(
            repo, "restore", "--target", target, "--input", str(archive), env=env
        )
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(reason, result.stderr)
        self.assertFalse((repo / f"sdks/{target}/dist").exists())

    def test_both_families_restore_full_packages_to_another_checkout(self):
        consumer = None
        for target in ("node", "browser"):
            with self.subTest(target=target):
                archive = self.stage(target)
                consumer = consumer or self.consumer()
                restored = self.command(
                    consumer, "restore", "--target", target, "--input", str(archive)
                )
                self.assertEqual(restored.returncode, 0, restored.stderr)
                self.assertTrue(
                    (
                        Path(restored.stdout.strip())
                        / ("typescript-napi" if target == "node" else "typescript-wasm")
                        / "sdk-contract.json"
                    ).is_file()
                )
                verified = self.command(consumer, "verify", "--target", target)
                self.assertEqual(verified.returncode, 0, verified.stderr)
                blockers = self.base / "forbidden-builds"
                blockers.mkdir(exist_ok=True)
                for command in ("node", "cargo", "nix"):
                    executable = blockers / command
                    executable.write_text(
                        "#!/bin/sh\necho forbidden compiler/stager >&2\nexit 99\n"
                    )
                    executable.chmod(0o755)
                prepared = subprocess.run(
                    ["bash", str(consumer / "dev/js/sdk-package"), target],
                    cwd=consumer,
                    env={
                        **self.env,
                        "XMTP_SDK_PREPARED_PRODUCTS": "1",
                        "XMTP_SDK_GENERATED_DIR": restored.stdout.strip(),
                        "PATH": str(blockers) + os.pathsep + self.env["PATH"],
                    },
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(prepared.returncode, 0, prepared.stderr)
                self.assertIn("SDK local imports verified", prepared.stdout)
                union = consumer / (target + "-generated-checks")
                shutil.copytree(Path(restored.stdout.strip()), union)
                raw_trees = (
                    ("typescript-napi",)
                    if target == "node"
                    else ("typescript-wasm", "typescript-pure")
                )
                linked = subprocess.run(
                    [
                        "bash",
                        str(consumer / "crates/xmtp_sdk/dev/link-runtime-packages"),
                        *[str(union / tree) for tree in raw_trees],
                    ],
                    cwd=consumer,
                    env={
                        **self.env,
                        "XMTP_SDK_RUNTIME_DIR": str(
                            Path(restored.stdout.strip()).parent / "runtimes"
                        ),
                        "PATH": str(blockers) + os.pathsep + self.env["PATH"],
                    },
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(linked.returncode, 0, linked.stderr)
                for tree in raw_trees:
                    self.assertTrue(
                        (union / tree / "node_modules/@ubjs/core/index.js").is_file()
                    )
                package = consumer / f"sdks/{target}/dist"
                self.assertTrue((package / "entry.d.ts").is_file())
                if target == "browser":
                    self.assertTrue((package / "typescript-wasm/worker.js").is_file())
                    self.assertTrue(
                        (package / "typescript-pure/xmtp_sdk_bg.wasm").is_file()
                    )
                self.assertTrue(
                    (package / "node_modules/@ubjs/core/index.js").is_file()
                )
                original = self.repo / f"target/sdk-packages/{target}"
                self.assertEqual(
                    {
                        p.relative_to(package): digest(p)
                        for p in package.rglob("*")
                        if p.is_file()
                    },
                    {
                        p.relative_to(original): digest(p)
                        for p in original.rglob("*")
                        if p.is_file()
                    },
                )
                asset = package / "entry.js"
                asset.write_text("changed workspace import")
                failed = self.command(consumer, "verify", "--target", target)
                self.assertNotEqual(failed.returncode, 0)
                self.assertIn("prepared package bytes mismatch", failed.stderr)

    def test_stale_source_and_generator_fail_at_current_checkout_guards(self):
        archive = self.stage("node")
        consumer = self.consumer()
        source = consumer / "crates/fixture/src/lib.rs"
        source.parent.mkdir(parents=True)
        source.write_text("pub fn changed() {}")
        self.assert_rejected(consumer, archive, "node", "current source mismatch")
        source.unlink()
        generator = consumer / "apps/xmtp_sdk_bindgen/template.txt"
        generator.parent.mkdir(parents=True)
        generator.write_text("changed generator")
        self.assert_rejected(consumer, archive, "node", "generator mismatch")
        generator.unlink()
        good = self.command(
            consumer, "restore", "--target", "node", "--input", str(archive)
        )
        self.assertEqual(good.returncode, 0, good.stderr)

    def test_wrong_context_and_same_run_identity_are_rejected(self):
        archive = self.stage("node")
        consumer = self.consumer()
        for key, value in (
            ("profile", "release"),
            ("target", "browser"),
            ("features", ["conformance"]),
            ("instrumentation", "coverage"),
            ("buildContextHash", "a" * 64),
            ("compilerIdentity", "different compiler"),
            ("host", "different-host"),
        ):
            with self.subTest(key=key):
                changed = self.mutate(
                    archive,
                    lambda _, manifest: manifest["context"].update({key: value}),
                )
                self.assert_rejected(consumer, changed, "node", "context mismatch")
        sha = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=consumer, text=True
        ).strip()
        env = {
            **self.env,
            "GITHUB_ACTIONS": "true",
            "GITHUB_RUN_ID": "123",
            "GITHUB_RUN_ATTEMPT": "1",
            "GITHUB_SHA": sha,
        }
        self.assert_rejected(
            consumer, archive, "node", "run identity mismatch", env=env
        )

    def test_same_run_products_pass_and_other_run_attempts_fail(self):
        archive = self.stage("node")
        sha = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.repo, text=True
        ).strip()
        env = {
            **self.env,
            "GITHUB_ACTIONS": "true",
            "GITHUB_RUN_ID": "123",
            "GITHUB_RUN_ATTEMPT": "2",
            "GITHUB_SHA": sha,
        }
        exported = self.command(
            self.repo, "export", "--target", "node", "--output", str(archive), env=env
        )
        self.assertEqual(exported.returncode, 0, exported.stderr)
        consumer = self.consumer()
        for key, value in (("GITHUB_RUN_ID", "456"), ("GITHUB_RUN_ATTEMPT", "3")):
            self.assert_rejected(
                consumer,
                archive,
                "node",
                "run identity mismatch",
                env={**env, key: value},
            )
        good = self.command(
            consumer, "restore", "--target", "node", "--input", str(archive), env=env
        )
        self.assertEqual(good.returncode, 0, good.stderr)

    def test_generated_receipts_reject_wrong_compile_semantics(self):
        archive = self.stage("node")
        consumer = self.consumer()
        for key, value, reason in (
            ("profile", "release", "profile mismatch"),
            ("features", "conformance", "features mismatch"),
            ("target", "aarch64-linux-android", "target mismatch"),
            ("instrumentation", "coverage", "debug compiler semantics mismatch"),
            (
                "profileOverrides",
                {"CARGO_PROFILE_DEV_DEBUG_ASSERTIONS": "false"},
                "debug compiler semantics mismatch",
            ),
            (
                "compilerFlags",
                {"RUSTFLAGS": "-Cpanic=abort"},
                "debug compiler semantics mismatch",
            ),
        ):
            with self.subTest(key=key):

                def update(folder, manifest):
                    path = folder / "generated/typescript-napi/sdk-contract.json"
                    receipt = json.loads(path.read_text())
                    receipt["artifact"][key] = value
                    path.write_text(json.dumps(receipt))
                    manifest["files"]["generated/typescript-napi/sdk-contract.json"] = (
                        digest(path)
                    )

                changed = self.mutate(archive, update)
                self.assert_rejected(consumer, changed, "node", reason)

    def test_changed_bytes_missing_runtime_and_archive_escape_are_rejected(self):
        archive = self.stage("browser")
        consumer = self.consumer()
        changed = self.mutate(
            archive,
            lambda folder, _: (folder / "package/entry.js").write_text(
                "changed artifact"
            ),
        )
        self.assert_rejected(consumer, changed, "browser", "product bytes mismatch")

        def drop_runtime(folder, manifest):
            for path in (folder / "package/node_modules/@ubjs/wasm").rglob("*"):
                if path.is_file():
                    name = path.relative_to(folder).as_posix()
                    del manifest["files"][name]
                    del manifest["modes"][name]
            shutil.rmtree(folder / "package/node_modules/@ubjs/wasm")
            # Update the staged receipt to reach the independent runtime guard.
            receipt_path = folder / "package/sdk-contract.json"
            receipt = json.loads(receipt_path.read_text())
            receipt["assets"] = {
                name: value
                for name, value in receipt["assets"].items()
                if not name.startswith("node_modules/@ubjs/wasm/")
            }
            receipt_path.write_text(json.dumps(receipt))
            manifest["files"]["package/sdk-contract.json"] = digest(receipt_path)

        changed = self.mutate(archive, drop_runtime)
        self.assert_rejected(
            consumer, changed, "browser", "required runtime missing: wasm"
        )
        escaped = self.base / "escape.tar"
        with tarfile.open(escaped, "w") as tar:
            info = tarfile.TarInfo("../escape")
            info.size = 0
            tar.addfile(info)
        self.assert_rejected(consumer, escaped, "browser", "unsafe archive path")


if __name__ == "__main__":
    unittest.main()
