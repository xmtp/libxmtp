#!/usr/bin/env python3
"""Check product guards before any runtime import or service startup."""

import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.dont_write_bytecode = True

TOOL = Path(__file__).with_name("backend-products.py")
REPO = TOOL.parents[2]
spec = importlib.util.spec_from_file_location("backend_products", TOOL)
products = importlib.util.module_from_spec(spec)
spec.loader.exec_module(products)
NATIVE = "/nix/store/" + "0" * 32 + "-backend"


class BackendProductTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = (Path(self.temp.name) / "checkout").resolve()
        self.root.mkdir()
        self.env = dict(os.environ, GITHUB_RUN_ID="81", GITHUB_RUN_ATTEMPT="2")
        self.env.pop("GITHUB_SHA", None)
        self.saved = dict(os.environ)
        os.environ.clear()
        os.environ.update(self.env)
        self.addCleanup(self.restore_environment)
        for name, data in {
            "Cargo.lock": "cargo pin",
            "flake.lock": "compiler pin",
            "Cargo.toml": "profile",
            "source.rs": "current backend",
            "dev/backend/local.toml": "current configuration",
            "nix/backend.nix": "build context",
        }.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(data)
        self.git("init", "-q")
        self.git("add", ".")
        self.git(
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "fixture",
        )
        self.stage = Path(self.temp.name) / "payload"
        self.stage.mkdir()
        config = json.dumps({"architecture": "amd64", "os": "linux"}).encode()
        with tarfile.open(self.stage / "image.tar", "w") as archive:
            for name, data in {
                "manifest.json": json.dumps(
                    [{"Config": "config.json", "RepoTags": [products.IMAGE]}]
                ).encode(),
                "config.json": config,
            }.items():
                entry = tarfile.TarInfo(name)
                entry.size = len(data)
                archive.addfile(entry, io.BytesIO(data))
        (self.stage / "runtime.export").write_bytes(b"fixture: not a real Nix closure")
        self.manifest = products.identity(self.root)
        self.manifest.update(
            runtimes={
                "nativeOutput": NATIVE,
                "closure": {NATIVE: "sha256-fixture"},
                "imageTag": products.IMAGE,
                "imageId": products.image_id(self.stage / "image.tar"),
                "binaryHash": "0" * 64,
            },
            files={
                p: products.digest(self.stage / p)
                for p in ("image.tar", "runtime.export")
            },
            modes={p: 0o644 for p in ("image.tar", "runtime.export")},
        )
        # Only host detection is replaced. Invalid products must fail before Nix.
        self.host = Path(self.temp.name) / "host"
        self.host.mkdir()
        (self.host / "sitecustomize.py").write_text(
            "import platform\nplatform.system=lambda: 'Linux'\nplatform.machine=lambda: 'x86_64'\n"
        )
        self.env["PYTHONPATH"] = str(self.host)
        self.archive = Path(self.temp.name) / "backend.tar"

    def restore_environment(self):
        os.environ.clear()
        os.environ.update(self.saved)

    def git(self, *args):
        return subprocess.run(
            ["git", "-C", str(self.root), *args], check=True, capture_output=True
        )

    def pack(self, manifest=None, extra=None):
        products.write_json(self.stage / "manifest.json", manifest or self.manifest)
        with tarfile.open(self.archive, "w") as archive:
            for name in sorted(products.PAYLOAD):
                archive.add(self.stage / name, arcname=name, recursive=False)
            if extra is not None:
                archive.addfile(extra)

    def cli(self, *args):
        return subprocess.run(
            [sys.executable, str(TOOL), "--root", str(self.root), *args],
            env=self.env,
            capture_output=True,
            text=True,
        )

    def rejected(self, message, *args):
        result = self.cli(*args)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(message, result.stderr)

    def test_valid_portable_payload_and_mode(self):
        products.write_json(self.stage / "manifest.json", self.manifest)
        self.assertEqual(
            products.validate_payload(self.root, self.stage, False)["runtimes"][
                "imageId"
            ],
            self.manifest["runtimes"]["imageId"],
        )
        (self.stage / "runtime.export").chmod(0o600)
        with self.assertRaisesRegex(ValueError, "mode differs"):
            products.validate_payload(self.root, self.stage, False)

    def test_restore_rejects_stale_source_and_service_configuration(self):
        self.pack()
        for name in (
            "source.rs",
            "dev/backend/local.toml",
            "Cargo.lock",
            "flake.lock",
            "nix/backend.nix",
        ):
            with self.subTest(input=name):
                path = self.root / name
                original = path.read_bytes()
                path.write_bytes(original + b" changed")
                self.rejected(
                    "sourceHash differs", "restore", "--input", str(self.archive)
                )
                path.write_bytes(original)

    def test_restore_rejects_each_compile_context_mismatch(self):
        for key, value in {
            "target": "aarch64",
            "profile": "debug",
            "features": ["extra"],
            "compilerIdentity": "stale compiler",
            "instrumentation": "coverage",
            "dependencyLocks": {},
            "imageTarget": "wrong target",
        }.items():
            with self.subTest(context=key):
                manifest = copy.deepcopy(self.manifest)
                manifest["context"][key] = value
                self.pack(manifest)
                self.rejected(
                    "context differs", "restore", "--input", str(self.archive)
                )

    def test_restore_rejects_run_checkout_and_generator_mismatch(self):
        for key, value in {
            "runId": 82,
            "runAttempt": 3,
            "checkoutSha": "1" * 40,
            "generatorHash": "0" * 64,
        }.items():
            with self.subTest(identity=key):
                manifest = copy.deepcopy(self.manifest)
                manifest[key] = value
                self.pack(manifest)
                self.rejected(key + " differs", "restore", "--input", str(self.archive))

    def test_earlier_attempt_is_explicit_and_never_crosses_run(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["runAttempt"] = 1
        products.write_json(self.stage / "manifest.json", manifest)
        self.assertEqual(
            products.validate_payload(self.root, self.stage, True)["runAttempt"], 1
        )
        self.pack(manifest)
        self.rejected("runAttempt differs", "restore", "--input", str(self.archive))
        for key, value, message in (
            ("runId", 99, "runId differs"),
            ("runAttempt", 3, "invalid producer runAttempt"),
        ):
            manifest[key] = value
            self.pack(manifest)
            self.rejected(
                message,
                "restore",
                "--input",
                str(self.archive),
                "--allow-earlier-attempt",
            )
            manifest[key] = self.manifest[key]

    def test_restore_rejects_changed_bytes_and_missing_runtime(self):
        self.pack()
        (self.stage / "runtime.export").write_bytes(b"damaged")
        self.pack()
        self.rejected(
            "bytes differ: runtime.export", "restore", "--input", str(self.archive)
        )
        manifest = copy.deepcopy(self.manifest)
        manifest["files"].pop("runtime.export")
        self.pack(manifest)
        self.rejected(
            "incomplete product file inventory", "restore", "--input", str(self.archive)
        )

    def test_restore_rejects_missing_closure_identity_before_import(self):
        manifest = copy.deepcopy(self.manifest)
        manifest["runtimes"]["closure"] = {}
        self.pack(manifest)
        self.rejected(
            "invalid native runtime identity", "restore", "--input", str(self.archive)
        )

    def test_restore_rejects_changed_image_bytes(self):
        with (self.stage / "image.tar").open("ab") as stream:
            stream.write(b"changed image")
        self.pack()
        self.rejected(
            "bytes differ: image.tar", "restore", "--input", str(self.archive)
        )

    def test_restore_rejects_archive_escape_and_link_before_import(self):
        for name, kind in (
            ("../escape", tarfile.REGTYPE),
            ("image.tar", tarfile.SYMTYPE),
        ):
            with self.subTest(member=name):
                extra = tarfile.TarInfo(name)
                extra.type = kind
                extra.linkname = "outside"
                self.pack(extra=extra)
                self.rejected(
                    "unexpected paths", "restore", "--input", str(self.archive)
                )
        self.pack()
        with tarfile.open(self.archive, "w") as archive:
            for name in sorted(products.PAYLOAD):
                if name == "runtime.export":
                    member = tarfile.TarInfo(name)
                    member.type = tarfile.SYMTYPE
                    member.linkname = "/etc/passwd"
                    archive.addfile(member)
                else:
                    archive.add(self.stage / name, arcname=name)
        self.rejected("unsafe member", "restore", "--input", str(self.archive))

    def test_prepared_product_cannot_move_to_another_checkout(self):
        self.pack()
        self.rejected(
            "another checkout",
            "verify-prepared",
            "--manifest",
            str(self.stage / "manifest.json"),
        )

    @unittest.skipUnless(shutil.which("just"), "requires pinned just from Nix")
    def test_up_prepared_rejects_stale_source_before_stack_start(self):
        (self.root / "apps/backend").mkdir(parents=True)
        shutil.copyfile(
            REPO / "apps/backend/backend.just", self.root / "apps/backend/backend.just"
        )
        (self.root / "justfile").write_text("mod backend 'apps/backend/backend.just'\n")
        (self.root / "dev/ci").mkdir(parents=True)
        shutil.copyfile(TOOL, self.root / "dev/ci/backend-products.py")
        (self.root / "dev/docker").mkdir(parents=True)
        up = self.root / "dev/docker/up"
        up.write_text('#!/bin/bash\ntouch "$STACK_STARTED"\n')
        up.chmod(0o755)
        self.git("add", ".")
        self.git(
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "commit",
            "-qm",
            "recipe fixture",
        )
        self.manifest.update(products.identity(self.root))
        prepared = self.root / "target/ci-products" / products.FAMILY
        shutil.copytree(self.stage, prepared)
        products.write_json(prepared / "manifest.json", self.manifest)
        products.write_json(
            prepared / "prepared.json",
            {
                "checkoutRoot": str(self.root),
                "manifestHash": products.digest(prepared / "manifest.json"),
                "allowEarlierAttempt": False,
            },
        )
        (self.root / "source.rs").write_text("new source")
        started = Path(self.temp.name) / "stack-started"
        env = dict(
            self.env,
            XMTP_CI_BACKEND_PREPARED="1",
            XMTP_CI_BACKEND_MANIFEST=str(prepared / "manifest.json"),
            STACK_STARTED=str(started),
        )
        result = subprocess.run(
            [
                "just",
                "--justfile",
                str(self.root / "justfile"),
                "backend",
                "up-prepared",
                "db",
                "replica",
            ],
            cwd=self.root,
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("sourceHash differs", result.stderr)
        self.assertFalse(started.exists(), "stale product started the stack")


if __name__ == "__main__":
    unittest.main()
