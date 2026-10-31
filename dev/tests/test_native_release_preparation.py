"""Prepare and reuse native tags with real Git and inert archive transport."""

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import yaml

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "dev" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


MIRRORS = load("sdk_version").MIRRORS
native = load("prepare_native_release")


class NativePreparationTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = dict(os.environ, GITHUB_REPOSITORY="xmtp/libxmtp")
        self.git("init", "--initial-branch=main")
        self.git("config", "commit.gpgSign", "false")
        for name in MIRRORS:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("source fixture\n")
        self.write("Package.swift", "fixture package\n")
        self.write("sdks/ios/Sources/XmtpSdk/Binding.swift", "generated binding\n")
        self.write("sdks/ios/ReleaseArtifacts.json", "{}\n")
        self.write(
            "dev/nix-shell",
            '#!/bin/bash\nif [ "$1" = --shell ]; then shift 2; fi\nif [ "$1" = --command ]; then shift; fi\nexec "$@"\n',
            executable=True,
        )
        for name in ("prepare_native_release.py", "sdk_version.py"):
            shutil.copyfile(ROOT / "dev" / name, self.root / "dev" / name)
        self.git("add", ".")
        self.git("commit", "-m", "synthetic frozen source")
        self.source = self.git("rev-parse", "HEAD")
        self.record = dict(
            sourceSha=self.source,
            targetVersion="8.0.0",
            version="8.0.0-rc1",
            kind="rc",
            timestamp="20261031000000",
            rcNumber=1,
        )
        self.runner = self.root / "runner"
        (self.runner / "sdk-release-plan").mkdir(parents=True)
        self.plan = self.runner / "sdk-release-plan/release-plan.json"
        self.plan.write_text(json.dumps(self.record))
        self.product = self.root / "target/release/ios"
        self.product.mkdir(parents=True)
        self.archive = self.product / "XmtpSdkFFI.zip"
        self.bundle = self.runner / "release.bundle"

    def git(self, *args):
        return subprocess.check_output(
            ["git", "-C", str(self.root), *args],
            env=self.env,
            text=True,
            stderr=subprocess.PIPE,
        ).strip()

    def write(self, name, text, executable=False):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        if executable:
            path.chmod(0o755)

    def zip(self, content=b"native bytes", year=2000):
        with zipfile.ZipFile(self.archive, "w") as archive:
            entry = zipfile.ZipInfo(
                "XmtpSdkFFI.xcframework/Info.plist", (year, 1, 1, 0, 0, 0)
            )
            archive.writestr(entry, content)

    def commit_product(self, content=b"native bytes", year=2000, **overrides):
        self.git("checkout", "--detach", self.source)
        self.zip(content, year)
        receipt = dict(
            url=f"https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-{self.record['version']}/XmtpSdkFFI.zip",
            sha256=hashlib.sha256(self.archive.read_bytes()).hexdigest(),
            sourceSha=self.source,
            version=self.record["version"],
        )
        receipt.update(overrides)
        self.write("sdks/ios/ReleaseArtifacts.json", json.dumps(receipt))
        self.git("add", "sdks/ios/ReleaseArtifacts.json")
        self.git("commit", "-m", "synthetic direct Apple release")
        return self.git("rev-parse", "HEAD")

    def prepare(self, sdk):
        with patch.dict(os.environ, self.env):
            native.prepare(self.root, sdk, self.record, self.bundle)

    def assert_bundle(self, sdk, expected):
        with tempfile.TemporaryDirectory() as folder:
            subprocess.run(
                ["git", "init", "--bare", folder], check=True, capture_output=True
            )
            ref = f"refs/tags/{sdk}-{self.record['version']}"
            subprocess.run(
                ["git", "--git-dir", folder, "fetch", str(self.bundle), f"{ref}:{ref}"],
                check=True,
                capture_output=True,
            )
            actual = subprocess.check_output(
                ["git", "--git-dir", folder, "rev-parse", f"{ref}^{{commit}}"],
                text=True,
            ).strip()
            self.assertEqual(actual, expected)

    def existing_ios(self, annotated=False):
        commit = self.commit_product()
        self.git(
            "tag",
            *(("-a", "-m", "synthetic annotated tag") if annotated else ()),
            "ios-8.0.0-rc1",
        )
        self.git("tag", "libxmtp-ios-8.0.0-rc1", self.source)
        data = self.archive.read_bytes()
        self.commit_product(year=2001)
        return commit, data

    def test_android_exact_boundary_creates_and_reuses_source_bound_tags(self):
        jobs = yaml.safe_load(
            (ROOT / ".github/workflows/release-android.yml").read_text()
        )["jobs"]
        command = next(
            s["run"]
            for s in jobs["build"]["steps"]
            if s.get("name") == "Prepare release tag"
        )
        for version, annotated in (("8.0.0-rc1", False), ("8.0.0-rc2", True)):
            self.record.update(version=version, rcNumber=2 if annotated else 1)
            self.plan.write_text(json.dumps(self.record))
            if annotated:
                self.git(
                    "tag", "-a", "-m", "synthetic annotated tag", f"android-{version}"
                )
            for attempt in range(2):
                result = subprocess.run(
                    ["bash", "-euc", command],
                    cwd=self.root,
                    env=self.env
                    | {"VERSION": version, "RUNNER_TEMP": str(self.runner)},
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(
                    self.git("rev-parse", f"refs/tags/android-{version}^{{commit}}"),
                    self.source,
                )
                self.assert_bundle("android", self.source)
        self.write("unrelated.txt", "wrong source\n")
        self.git("add", "unrelated.txt")
        self.git("commit", "-m", "synthetic wrong source")
        self.git("tag", "android-8.0.0-rc3")
        self.git("checkout", "--detach", self.source)
        self.record.update(version="8.0.0-rc3", rcNumber=3)
        self.plan.write_text(json.dumps(self.record))
        result = subprocess.run(
            ["bash", "-euc", command],
            cwd=self.root,
            env=self.env
            | {"VERSION": self.record["version"], "RUNNER_TEMP": str(self.runner)},
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("frozen source", result.stderr)

    def test_ios_exact_packaging_boundary_reuses_a_valid_release(self):
        jobs = yaml.safe_load((ROOT / ".github/workflows/release-ios.yml").read_text())[
            "jobs"
        ]
        steps = jobs["prepare-release"]["steps"]
        command = next(
            s["run"]
            for s in steps
            if s.get("name") == "Package the checked SwiftPM archive"
        )
        self.write(
            "build/ios/XmtpSdkFFI/Sources/XmtpSdk/Binding.swift", "generated binding\n"
        )
        self.write(
            "build/ios/XmtpSdkFFI/XmtpSdkFFI.xcframework/Info.plist", "native bytes\n"
        )
        env = self.env | {
            "VERSION": self.record["version"],
            "SOURCE_SHA": self.source,
            "RUNNER_TEMP": str(self.runner),
        }
        result = subprocess.run(
            ["bash", "-euc", command],
            cwd=self.root,
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        tag = self.git("rev-parse", "refs/tags/ios-8.0.0-rc1^{commit}")
        published = self.archive.read_bytes()
        self.git("tag", "libxmtp-ios-8.0.0-rc1", self.source)
        self.git("checkout", "--detach", self.source)
        self.write(
            "dev/archive-transport.py",
            'import io, os, runpy, sys\nfrom pathlib import Path\nfrom unittest.mock import patch\nscript = sys.argv.pop(1)\ndef read(url):\n assert url == "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-8.0.0-rc1/XmtpSdkFFI.zip"\n return io.BytesIO(Path(os.environ["ARCHIVE_FIXTURE"]).read_bytes())\nwith patch("urllib.request.urlopen", read): runpy.run_path(script, run_name="__main__")\n',
        )
        existing = self.runner / "published.zip"
        existing.write_bytes(published)
        self.write(
            "dev/nix-shell",
            '#!/bin/bash\nif [ "$1" = --shell ]; then shift 2; fi\nif [ "$1" = --command ]; then shift; fi\nif [ "$2" = dev/prepare_native_release.py ]; then exec "$1" dev/archive-transport.py "${@:2}"; fi\nexec "$@"\n',
            executable=True,
        )
        result = subprocess.run(
            ["bash", "-euc", command],
            cwd=self.root,
            env=env | {"ARCHIVE_FIXTURE": str(existing)},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.git("rev-parse", "refs/tags/ios-8.0.0-rc1^{commit}"), tag)
        self.assertEqual(self.archive.read_bytes(), published)
        for name in (
            "Build XCFramework and generated Swift sources",
            "Install the prepared SwiftPM release archive",
        ):
            self.assertNotIn("if", next(s for s in steps if s.get("name") == name))

    def test_ios_reuses_exact_annotated_archive_with_different_zip_timestamps(self):
        previous, published = self.existing_ios(annotated=True)
        self.assertNotEqual(self.archive.read_bytes(), published)
        with patch.object(
            native, "urlopen", return_value=io.BytesIO(published)
        ) as request:
            self.prepare("ios")
        self.assertEqual(
            request.call_args.args,
            (
                "https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-8.0.0-rc1/XmtpSdkFFI.zip",
            ),
        )
        self.assertEqual(self.archive.read_bytes(), published)
        self.assertEqual(
            self.git("rev-parse", "refs/tags/ios-8.0.0-rc1^{commit}"), previous
        )
        self.assert_bundle("ios", previous)

    def test_ios_rejects_wrong_identity_and_release_tree(self):
        for overrides in (
            {"version": "8.0.0-rc2"},
            {"sourceSha": "a" * 40},
            {"url": "https://invalid.example/archive.zip"},
            {"sha256": "bad"},
        ):
            with self.subTest(overrides=overrides):
                self.commit_product(**overrides)
                with self.assertRaises(ValueError):
                    self.prepare("ios")
        self.commit_product()
        self.write("unrelated.txt", "not a release path\n")
        self.git("add", "unrelated.txt")
        self.git("commit", "--amend", "--no-edit")
        with self.assertRaisesRegex(ValueError, "outside release paths"):
            self.prepare("ios")

    def test_ios_rejects_inconsistent_and_malformed_existing_products(self):
        previous, published = self.existing_ios()
        for data in (b"malformed archive", published + b"changed bytes"):
            with (
                self.subTest(data=data[-12:]),
                patch.object(native, "urlopen", return_value=io.BytesIO(data)),
            ):
                with self.assertRaisesRegex(
                    ValueError, "differs from its release receipt"
                ):
                    self.prepare("ios")
        self.commit_product(content=b"different native bytes", year=2001)
        with patch.object(native, "urlopen", return_value=io.BytesIO(published)):
            with self.assertRaisesRegex(ValueError, "fresh native build"):
                self.prepare("ios")
        self.assertEqual(
            self.git("rev-parse", "refs/tags/ios-8.0.0-rc1^{commit}"), previous
        )
        self.archive.write_bytes(b"not a ZIP")
        self.write(
            "sdks/ios/ReleaseArtifacts.json",
            json.dumps(
                dict(
                    url="https://github.com/xmtp/libxmtp/releases/download/libxmtp-ios-8.0.0-rc1/XmtpSdkFFI.zip",
                    sha256=hashlib.sha256(self.archive.read_bytes()).hexdigest(),
                    sourceSha=self.source,
                    version=self.record["version"],
                )
            ),
        )
        self.git("add", "sdks/ios/ReleaseArtifacts.json")
        self.git("commit", "--amend", "--no-edit")
        with self.assertRaises(zipfile.BadZipFile):
            self.prepare("ios")

    def test_ios_rejects_existing_receipt_identity_conflicts(self):
        for number, overrides in enumerate(
            (
                {"version": "8.0.0-rc99"},
                {"sourceSha": "a" * 40},
                {"url": "https://invalid.example/archive.zip"},
                {"sha256": "bad"},
            ),
            start=1,
        ):
            with self.subTest(overrides=overrides):
                self.record.update(version=f"8.0.0-rc{number}", rcNumber=number)
                previous = self.commit_product(**overrides)
                self.git("tag", f"ios-{self.record['version']}")
                self.git("tag", f"libxmtp-ios-{self.record['version']}", self.source)
                self.commit_product(year=2001)
                with patch.object(native, "urlopen") as request:
                    with self.assertRaisesRegex(ValueError, "receipt differs"):
                        self.prepare("ios")
                    request.assert_not_called()
                self.assertEqual(
                    self.git(
                        "rev-parse",
                        f"refs/tags/ios-{self.record['version']}^{{commit}}",
                    ),
                    previous,
                )

    def test_ios_rejects_wrong_tag_parent_binary_source_and_generated_sources(self):
        self.commit_product()
        self.git("commit", "--allow-empty", "-m", "synthetic wrong parent")
        self.git("tag", "ios-8.0.0-rc1")
        self.commit_product(year=2001)
        with self.assertRaisesRegex(ValueError, "frozen source"):
            self.prepare("ios")
        self.record.update(version="8.0.0-rc2", rcNumber=2)
        previous = self.commit_product()
        self.git("tag", "ios-8.0.0-rc2")
        self.git("tag", "libxmtp-ios-8.0.0-rc2", previous)
        self.commit_product(year=2001)
        with self.assertRaisesRegex(ValueError, "binary tag differs"):
            self.prepare("ios")
        self.record.update(version="8.0.0-rc3", rcNumber=3)
        self.commit_product()
        self.git("tag", "ios-8.0.0-rc3")
        self.git("tag", "libxmtp-ios-8.0.0-rc3", self.source)
        self.commit_product(year=2001)
        self.write(
            "sdks/ios/Sources/XmtpSdk/Binding.swift", "conflicting generated source\n"
        )
        self.git("add", "sdks/ios/Sources/XmtpSdk/Binding.swift")
        self.git("commit", "--amend", "--no-edit")
        with self.assertRaises(subprocess.CalledProcessError):
            self.prepare("ios")


if __name__ == "__main__":
    unittest.main()
