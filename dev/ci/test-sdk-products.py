#!/usr/bin/env python3
"""Exercise portable SDK transport through the public staging and CLI paths."""

import hashlib
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import struct
import sys
import tarfile
import tempfile
import unittest

from sdk_response_flag_test_cases import ResponseFlagTests

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = "dev/ci/sdk-products.py"
STAGE = "crates/xmtp_sdk/dev/stage-package.mjs"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def native_elf(search_path):
    """An independent ELF load-table fixture; this is not an ABI load proof."""
    strings = b"\0libfixture.so\0" + search_path.encode() + b"\0"
    dynamic = ((5, 0x200), (10, len(strings)), (1, 1), (29, 15), (0, 0))
    data = bytearray(0x200 + len(strings))
    data[:16] = b"\x7fELF\x02\x01\x01" + b"\0" * 9
    struct.pack_into(
        "<HHIQQQIHHHHHH", data, 16, 3, 62, 1, 0, 64, 0, 0, 64, 56, 2, 0, 0, 0
    )
    struct.pack_into("<IIQQQQQQ", data, 64, 1, 4, 0, 0, 0, len(data), len(data), 4096)
    struct.pack_into(
        "<IIQQQQQQ",
        data,
        120,
        2,
        4,
        0x100,
        0x100,
        0,
        len(dynamic) * 16,
        len(dynamic) * 16,
        8,
    )
    for index, (tag, value) in enumerate(dynamic):
        struct.pack_into("<qQ", data, 0x100 + index * 16, tag, value)
    data[0x200:] = strings
    return data


class ProductTests(ResponseFlagTests, unittest.TestCase):
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
            "dev/ci/sdk-native-runtime.py",
            "crates/xmtp_sdk/dev/sdk-artifacts.py",
            "crates/xmtp_sdk/dev/sdk-build-inputs.py",
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
            [sys.executable, "-B", str(root / SCRIPT), *args],
            cwd=root,
            env=env or self.env,
            capture_output=True,
            text=True,
        )

    def stage(self, target, native_path=None):
        generated = self.repo / "target/sdk-generated"
        # Use the common receipt producer's source hashes, then let real
        # stage-package.mjs validate bytes and produce its own final receipt.
        hashes = subprocess.check_output(
            [
                sys.executable,
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
                library.write_bytes(
                    native_elf(native_path)
                    if native_path
                    else b"fixture SDK shared library"
                )
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
                "debugProfileValid": True,
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
                for command in ("node", "cargo", "nix", "python3"):
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

    def test_same_run_reruns_reuse_earlier_products_and_reject_future_attempts(self):
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
        for key, value in (("GITHUB_RUN_ID", "456"), ("GITHUB_RUN_ATTEMPT", "1")):
            self.assert_rejected(
                consumer,
                archive,
                "node",
                "run identity mismatch",
                env={**env, key: value},
            )
        for field, value in (
            ("runAttempt", 0),
            ("runAttempt", True),
            ("checkoutSha", "f" * 40),
        ):
            changed = self.mutate(
                archive, lambda _, manifest: manifest.update({field: value})
            )
            self.assert_rejected(
                consumer, changed, "node", "run identity mismatch", env=env
            )
        for attempt in ("2", "3"):
            environment = {**env, "GITHUB_RUN_ATTEMPT": attempt}
            good = self.command(
                consumer,
                "restore",
                "--target",
                "node",
                "--input",
                str(archive),
                env=environment,
            )
            self.assertEqual(good.returncode, 0, good.stderr)
            verified = self.command(
                consumer, "verify", "--target", "node", env=environment
            )
            self.assertEqual(verified.returncode, 0, verified.stderr)

    def test_producer_selects_rust_even_with_default_caller(self):
        marker = self.base / "generation-shell.json"
        launcher = self.repo / "dev/nix-shell"
        launcher.write_text(
            f"#!{sys.executable}\n"
            "import json, sys\n"
            "from pathlib import Path\n"
            f"Path({str(marker)!r}).write_text(json.dumps(sys.argv[1:]))\n"
            "raise SystemExit(99)\n"
        )
        launcher.chmod(0o755)
        for target in ("node", "browser"):
            with self.subTest(target=target):
                result = self.command(
                    self.repo,
                    "build",
                    "--target",
                    target,
                    "--output",
                    str(self.base / f"{target}.tar"),
                    env={**self.env, "NIX_DEVSHELL": "default"},
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(
                    json.loads(marker.read_text())[:2], ["--shell", "rust"]
                )
                marker.unlink()

    def test_all_false_debug_flag_forms_fail_before_generation(self):
        marker = self.base / "generation-called"
        launcher = self.repo / "dev/nix-shell"
        launcher.write_text(
            "#!/bin/sh\necho generation >> " + str(marker) + "\nexit 99\n"
        )
        launcher.chmod(0o755)
        output = self.base / "must-not-exist.tar"
        for name in (
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "CARGO_HOST_RUSTFLAGS",
        ):
            for value in ("n", "no", "off", "false"):
                for separated in (False, True):
                    separator = "\x1f" if name == "CARGO_ENCODED_RUSTFLAGS" else " "
                    flag = (
                        "-C"
                        + (separator if separated else "")
                        + "debug-assertions="
                        + value
                    )
                    failed = self.command(
                        self.repo,
                        "build",
                        "--target",
                        "node",
                        "--output",
                        str(output),
                        env={**self.env, name: flag},
                    )
                    self.assertNotEqual(failed.returncode, 0)
                    self.assertIn("default debug compiler profile", failed.stderr)
                    self.assertFalse(marker.exists())
                    self.assertFalse(output.exists())
        for flag in ("", "-Cdebug-assertions=true", "-C debug-assertions=yes"):
            allowed = self.command(
                self.repo,
                "build",
                "--target",
                "node",
                "--output",
                str(output),
                env={**self.env, "RUSTFLAGS": flag},
            )
            self.assertTrue(marker.is_file(), allowed.stderr)
            marker.unlink()

    def test_cargo_config_flags_and_profiles_fail_before_generation(self):
        marker = self.base / "generation-called"
        launcher = self.repo / "dev/nix-shell"
        launcher.write_text(
            "#!/bin/sh\necho generation >> " + str(marker) + "\nexit 99\n"
        )
        launcher.chmod(0o755)
        folder = self.repo / ".cargo"
        folder.mkdir()
        cases = (
            '[build]\nrustflags=["-Cdebug-assertions=false"]\n',
            '[target."cfg(all())"]\nrustflags=["-C","debug-assertions=off"]\n',
            "[profile.dev]\ndebug-assertions=false\n",
            "[profile.dev.package.xmtp_sdk]\ndebug-assertions=false\n",
            '[env]\nCARGO_BUILD_RUSTFLAGS="-Cpanic=abort"\n',
            '[env]\nCARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS={value="-Copt-level=3",force=true}\n',
            '[env]\nCARGO_PROFILE_DEV_DEBUG_ASSERTIONS="false"\n',
            '[host]\nrustflags=["-Cpanic=abort"]\n',
        )
        for body in cases:
            (folder / "config").write_text(body)
            failed = self.command(
                self.repo,
                "build",
                "--target",
                "node",
                "--output",
                str(self.base / "invalid.tar"),
            )
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn("default debug compiler profile", failed.stderr)
            self.assertFalse(marker.exists())

    def test_target_and_build_flags_reject_panic_and_optimization_before_generation(
        self,
    ):
        marker = self.base / "generation-called"
        launcher = self.repo / "dev/nix-shell"
        launcher.write_text(
            "#!/bin/sh\necho generation >> " + str(marker) + "\nexit 99\n"
        )
        launcher.chmod(0o755)
        for name in (
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
        ):
            for flag in (
                "-Cpanic=abort",
                "-C opt-level=3",
                "--codegen=debug-assertions=false",
                "--codegen panic=abort",
            ):
                failed = self.command(
                    self.repo,
                    "build",
                    "--target",
                    "node",
                    "--output",
                    str(self.base / "invalid.tar"),
                    env={**self.env, name: flag},
                )
                self.assertNotEqual(failed.returncode, 0)
                self.assertIn("default debug compiler profile", failed.stderr)
                self.assertFalse(marker.exists())
        for name in (
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
        ):
            allowed = self.command(
                self.repo,
                "build",
                "--target",
                "node",
                "--output",
                str(self.base / "normal.tar"),
                env={**self.env, name: "-Cdebug-assertions=true"},
            )
            self.assertTrue(marker.is_file(), allowed.stderr)
            marker.unlink()

        (self.repo / "Cargo.toml").write_text('[profile.dev]\npanic="abort"\n')
        failed = self.command(
            self.repo,
            "build",
            "--target",
            "node",
            "--output",
            str(self.base / "invalid.tar"),
        )
        self.assertIn("default debug compiler profile", failed.stderr)
        self.assertFalse(marker.exists())

    def test_encoded_argument_boundaries_at_generation_and_promotion_guards(self):
        archive = self.stage("node")
        consumer = self.consumer()
        arguments = ["--cfg", 'probe="not -Copt-level=3"']
        for flags, rejected in (
            (arguments + ["-C", "debug-assertions=false"], True),
            (arguments, False),
        ):

            def update(folder, manifest):
                path = folder / "generated/typescript-napi/sdk-contract.json"
                receipt = json.loads(path.read_text())
                receipt["artifact"]["compilerFlags"] = {
                    "CARGO_ENCODED_RUSTFLAGS": flags
                }
                path.write_text(json.dumps(receipt))
                manifest["files"]["generated/typescript-napi/sdk-contract.json"] = (
                    digest(path)
                )

            changed = self.mutate(archive, update)
            if rejected:
                self.assert_rejected(
                    consumer, changed, "node", "debug compiler semantics mismatch"
                )
            else:
                restored = self.command(
                    consumer, "restore", "--target", "node", "--input", str(changed)
                )
                self.assertEqual(restored.returncode, 0, restored.stderr)

        marker = self.base / "generation-called"
        launcher = self.repo / "dev/nix-shell"
        launcher.write_text(
            "#!/bin/sh\necho generation >> " + str(marker) + "\nexit 99\n"
        )
        launcher.chmod(0o755)
        for route in ("environment", "config-env"):
            for flags, rejected in (
                (arguments, False),
                (arguments + ["-C", "debug-assertions=false"], True),
            ):
                environment = dict(self.env)
                encoded = "\x1f".join(flags)
                if route == "environment":
                    environment["CARGO_ENCODED_RUSTFLAGS"] = encoded
                else:
                    (self.repo / ".cargo").mkdir(exist_ok=True)
                    (self.repo / ".cargo/config").write_text(
                        "[env]\nCARGO_ENCODED_RUSTFLAGS=" + json.dumps(encoded) + "\n"
                    )
                result = self.command(
                    self.repo,
                    "build",
                    "--target",
                    "node",
                    "--output",
                    str(self.base / "encoded.tar"),
                    env=environment,
                )
                if rejected:
                    self.assertIn("default debug compiler profile", result.stderr)
                    self.assertFalse(marker.exists())
                else:
                    self.assertTrue(marker.is_file(), result.stderr)
                    marker.unlink()

    def test_false_flags_in_restored_receipts_reject_before_promotion(self):
        archive = self.stage("node")
        consumer = self.consumer()
        for name in (
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "CARGO_HOST_RUSTFLAGS",
        ):
            for value in ("n", "no", "off", "false"):
                separator = "\x1f" if name == "CARGO_ENCODED_RUSTFLAGS" else " "
                flag = "-C" + separator + "debug-assertions=" + value

                def update(folder, manifest):
                    path = folder / "generated/typescript-napi/sdk-contract.json"
                    receipt = json.loads(path.read_text())
                    receipt["artifact"]["compilerFlags"] = {name: flag}
                    path.write_text(json.dumps(receipt))
                    manifest["files"]["generated/typescript-napi/sdk-contract.json"] = (
                        digest(path)
                    )

                changed = self.mutate(archive, update)
                self.assert_rejected(
                    consumer, changed, "node", "debug compiler semantics mismatch"
                )

    def test_explicit_native_scan_paths_cover_executables_and_reject_escape(self):
        spec = __import__("importlib.util", fromlist=["util"]).spec_from_file_location(
            "native_runtime", ROOT / "dev/ci/sdk-native-runtime.py"
        )
        runtime = __import__("importlib.util", fromlist=["util"]).module_from_spec(spec)
        spec.loader.exec_module(runtime)
        tree = self.base / "mapping"
        (tree / "tests").mkdir(parents=True)
        executable = tree / "tests/test-binary"
        executable.write_bytes(native_elf(""))
        (tree / "proc.so").write_bytes(native_elf(""))
        record = runtime.export(tree, self.base, ["tests/test-binary", "proc.so"])
        self.assertEqual(record["scanPaths"], ["proc.so", "tests/test-binary"])
        self.assertEqual(set(record["linkage"]), set(record["scanPaths"]))
        runtime.check(tree, self.base, record)
        for path in ("../escape", str(executable), "missing", "tests"):
            with self.assertRaises(ValueError):
                runtime.inspect(tree, [path])

    def test_native_closure_import_follows_complete_preflight(self):
        store = "/nix/store/" + "a" * 32 + "-fixture-native-runtime"
        tools = self.base / "store-tools"
        tools.mkdir()
        log = self.base / "store.log"
        imported = self.base / "imported"
        command = tools / "nix-store"
        command.write_text(
            "#!" + sys.executable + "\n"
            "import json,os,pathlib,sys\n"
            "args=sys.argv[1:]\n"
            "with open(os.environ['XMTP_FIXTURE_STORE_LOG'],'a') as f: f.write(json.dumps(args)+'\\n')\n"
            "if args[:2] == ['--query','--requisites']: print(" + repr(store) + ")\n"
            "elif args[:2] == ['--query','--hash']:\n"
            " if os.environ.get('XMTP_FIXTURE_STORE_MISSING'): sys.exit(7)\n"
            " if os.environ.get('XMTP_FIXTURE_STORE_REQUIRE_IMPORT') and not pathlib.Path(os.environ['XMTP_FIXTURE_STORE_IMPORTED']).exists(): sys.exit(7)\n"
            " for path in args[2:]: print('sha256:controlled-native-store')\n"
            "elif args == ['--import']:\n"
            " assert sys.stdin.buffer.read() == b'controlled-nix-export'\n"
            " pathlib.Path(os.environ['XMTP_FIXTURE_STORE_IMPORTED']).write_text('imported')\n"
            "elif args[0] == '--export': sys.stdout.buffer.write(b'controlled-nix-export')\n"
            "elif args[0] == '--verify-path':\n"
            " if os.environ.get('XMTP_FIXTURE_STORE_CONTENT_MISSING'): sys.exit(7)\n"
            "elif args[0] == '--realise':\n"
            " path=pathlib.Path(args[args.index('--add-root')+1])\n"
            " if path.is_symlink(): path.unlink()\n"
            " path.symlink_to(args[-1])\n"
            "else: sys.exit(9)\n"
        )
        command.chmod(0o755)
        self.env.update(
            {
                "PATH": str(tools) + os.pathsep + self.env["PATH"],
                "XMTP_FIXTURE_STORE_LOG": str(log),
                "XMTP_FIXTURE_STORE_IMPORTED": str(imported),
            }
        )
        archive = self.stage("node", store + "/lib")
        with tarfile.open(archive) as tar:
            manifest = json.load(tar.extractfile("manifest.json"))
        self.assertEqual(manifest["nativeRuntime"]["roots"], [store])
        self.assertEqual(
            manifest["nativeRuntime"]["linkage"]["libxmtp_sdk.so"],
            {
                "needed": ["libfixture.so"],
                "searchPaths": [store + "/lib"],
                "interpreter": None,
            },
        )
        self.assertIn("native-runtime.nar", manifest["files"])
        consumer = self.consumer()
        environment = {**self.env, "XMTP_FIXTURE_STORE_REQUIRE_IMPORT": "1"}
        log.write_text("")
        changed = self.mutate(
            archive,
            lambda folder, _: (folder / "native-runtime.nar").write_bytes(
                b"changed closure"
            ),
        )
        self.assert_rejected(
            consumer, changed, "node", "product bytes mismatch", env=environment
        )
        self.assertFalse(imported.exists())
        self.assertEqual(log.read_text(), "")
        changed = self.mutate(
            archive, lambda _, manifest: manifest["nativeRuntime"].update(roots=[])
        )
        self.assert_rejected(
            consumer,
            changed,
            "node",
            "native runtime ELF load inputs mismatch",
            env=environment,
        )
        self.assertFalse(imported.exists())
        self.assertEqual(log.read_text(), "")
        self.assert_rejected(
            consumer,
            archive,
            "node",
            "returned non-zero exit status 7",
            env={**environment, "XMTP_FIXTURE_STORE_MISSING": "1"},
        )
        self.assertFalse(
            any(
                args[0] == "--realise"
                for args in map(json.loads, log.read_text().splitlines())
            )
        )
        log.write_text("")
        self.assert_rejected(
            consumer,
            archive,
            "node",
            "returned non-zero exit status 7",
            env={**environment, "XMTP_FIXTURE_STORE_CONTENT_MISSING": "1"},
        )
        self.assertFalse(
            any(
                args[0] == "--realise"
                for args in map(json.loads, log.read_text().splitlines())
            )
        )
        log.write_text("")
        imported.unlink()
        good = self.command(
            consumer,
            "restore",
            "--target",
            "node",
            "--input",
            str(archive),
            env=environment,
        )
        self.assertEqual(good.returncode, 0, good.stderr)
        self.assertTrue(imported.is_file())
        roots = (
            consumer / "target/ci-products/.native-runtime-roots" / manifest["family"]
        )
        self.assertTrue((roots / Path(store).name).is_symlink())
        calls = list(map(json.loads, log.read_text().splitlines()))
        realised = next(
            index for index, args in enumerate(calls) if args[0] == "--realise"
        )
        self.assertTrue(
            any(args[:2] == ["--query", "--hash"] for args in calls[:realised])
        )
        self.assertTrue(any(args[0] == "--verify-path" for args in calls[:realised]))
        self.assertEqual(
            [
                args
                for args in map(json.loads, log.read_text().splitlines())
                if args == ["--import"]
            ],
            [["--import"]],
        )
        verified = self.command(consumer, "verify", "--target", "node", env=environment)
        self.assertEqual(verified.returncode, 0, verified.stderr)

    def test_generated_receipts_reject_wrong_compile_semantics(self):
        archive = self.stage("node")
        consumer = self.consumer()
        for key, value, reason in (
            ("debugProfileValid", False, "debug compiler semantics mismatch"),
            (
                "compilerFlags",
                {"CARGO_ENCODED_RUSTFLAGS": "-C\x1fdebug-assertions=false"},
                "debug compiler semantics mismatch",
            ),
            (
                "compilerFlags",
                {"RUSTFLAGS": "-C debug-assertions=off"},
                "debug compiler semantics mismatch",
            ),
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
