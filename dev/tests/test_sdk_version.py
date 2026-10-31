"""The real version CLI rejects drift and materializes versions before compilation."""

import importlib.machinery
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader(
    "sdk_version", str(ROOT / "dev/sdk_version.py")
)
spec = importlib.util.spec_from_loader(loader.name, loader)
version = importlib.util.module_from_spec(spec)
loader.exec_module(version)


class SdkVersionTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            "crates/xmtp_sdk/Cargo.toml",
            "sdks/android/gradle.properties",
            "apps/cli/package.json",
            *version.MANIFESTS,
        ]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((ROOT / name).read_bytes())
        version.sync(self.root, "8.0.0")
        self.git("init", "-q")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "user.name", "fixture")
        self.git("config", "commit.gpgSign", "false")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        self.sha = self.git("rev-parse", "HEAD")

    def git(self, *args):
        return subprocess.check_output(
            ["git", "-C", str(self.root), *args], text=True
        ).strip()

    def record(self, kind="rc", rc=5):
        base = version.target(self.root)
        return dict(
            sourceSha=self.sha,
            targetVersion=base,
            kind=kind,
            rcNumber=rc,
            timestamp="20261021153000",
            version=version.release_version(base, kind, self.sha, "20261021153000", rc),
        )

    def test_checker_rejects_each_independent_sdk_mirror(self):
        for name in [*version.MANIFESTS, "apps/cli/package.json"]:
            with self.subTest(name=name):
                path = self.root / name
                old = path.read_text()
                package = json.loads(old)
                package["version"] = "99.0.0"
                path.write_text(json.dumps(package))
                with self.assertRaisesRegex(ValueError, name):
                    version.check(self.root)
                path.write_text(old)
        path = self.root / "sdks/android/gradle.properties"
        path.write_text("version=99.0.0\n")
        with self.assertRaisesRegex(ValueError, "gradle.properties"):
            version.check(self.root)

    def test_crate_cannot_opt_out_of_inheritance(self):
        path = self.root / "crates/xmtp_sdk/Cargo.toml"
        path.write_text(
            path.read_text().replace("version.workspace = true", 'version = "8.0.0"')
        )
        with self.assertRaisesRegex(ValueError, "inherit"):
            version.check(self.root)

    def test_materialization_updates_cargo_locks_mirrors_and_agent_pin(self):
        plan = self.record()
        version.materialize(self.root, plan)
        self.assertEqual(version.target(self.root), "8.0.0-rc5")
        self.assertEqual(version.check(self.root), "8.0.0-rc5")
        for name in [*version.MANIFESTS, "apps/cli/package.json"]:
            self.assertEqual(
                json.loads((self.root / name).read_text())["version"], "8.0.0-rc5"
            )
        agent = json.loads((self.root / "sdks/agent/package.json").read_text())
        self.assertEqual(agent["dependencies"]["@xmtp/node-sdk"], "workspace:*")
        # Independent expected lock value, consumed by Cargo --locked.
        self.assertIn(
            'name = "xmtp_sdk"\nversion = "8.0.0-rc5"',
            (self.root / "Cargo.lock").read_text(),
        )
        version.materialize(self.root, plan)  # a retry retains the same value

    def test_plan_cannot_be_reused_with_changed_source_or_version(self):
        plan = self.record()
        with self.assertRaisesRegex(ValueError, "frozen release source"):
            version.materialize(self.root, plan | {"sourceSha": "a" * 40})
        with self.assertRaisesRegex(ValueError, "frozen plan"):
            version.materialize(self.root, plan | {"version": "8.0.0-rc6"})

    def test_ordinary_materialization_rejects_changed_tracked_source(self):
        source = self.root / "crates/xmtp_sdk/src/lib.rs"
        source.parent.mkdir()
        source.write_text("pub fn source() -> u8 { 1 }\n")
        self.git("add", str(source))
        self.git("commit", "-qm", "tracked source")
        self.sha = self.git("rev-parse", "HEAD")
        plan = self.record()
        source.write_text("pub fn source() -> u8 { 2 }\n")
        path = self.root / "release-plan.json"
        path.write_text(json.dumps(plan))
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "dev/sdk_version.py"),
                "--root",
                str(self.root),
                "materialize",
                "--plan",
                str(path),
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("changed tracked source", result.stderr)
        self.assertEqual(version.target(self.root), "8.0.0")
        self.assertEqual(source.read_text(), "pub fn source() -> u8 { 2 }\n")

    def test_materialization_retry_rejects_nonversion_mirror_changes(self):
        plan = self.record()
        version.materialize(self.root, plan)
        path = self.root / "sdks/node/package.json"
        package = json.loads(path.read_text())
        package["scripts"]["unexpected"] = "unreviewed-build-tool"
        path.write_text(json.dumps(package, indent=2) + "\n")
        with self.assertRaisesRegex(ValueError, "changed tracked source"):
            version.materialize(self.root, plan)
        self.assertEqual(version.target(self.root), "8.0.0-rc5")
        self.assertEqual(
            json.loads(path.read_text())["scripts"]["unexpected"],
            "unreviewed-build-tool",
        )

    def test_node_snapshot_admits_versioned_inputs_without_reusing_another_version(
        self,
    ):
        spec = importlib.util.spec_from_file_location(
            "sdk_artifacts", ROOT / "crates/xmtp_sdk/dev/sdk-artifacts.py"
        )
        artifacts = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(artifacts)
        keys = set()
        for kind, rc in [("dev", None), ("rc", 5), ("final", None)]:
            with self.subTest(kind=kind), patch.object(artifacts, "ROOT", self.root):
                self.git("checkout", "-q", "--detach", self.sha)
                plan = self.record(kind, rc)
                path = self.root / "release-plan.json"
                path.write_text(json.dumps(plan))
                command = [
                    sys.executable,
                    str(ROOT / "dev/sdk_version.py"),
                    "--root",
                    str(self.root),
                    "materialize",
                    "--snapshot",
                    "--plan",
                    str(path),
                ]
                subprocess.run(command, check=True, capture_output=True)
                snapshot = self.git("rev-parse", "HEAD")
                self.assertEqual(version.check(self.root), plan["version"])
                self.assertEqual(self.git("status", "--porcelain", "-uno"), "")
                # These are the actual Windows cache and build provenance
                # readers; the recorder rejects a +dirty identity.
                keys.add(artifacts.source_key()["source"])
                with patch.object(artifacts.sys, "platform", "win32"):
                    self.assertEqual(
                        artifacts.provenance.__wrapped__()["source"],
                        "commit:" + snapshot,
                    )
                self.assertEqual(json.loads(path.read_text()), plan)
                if kind != "final":
                    self.assertEqual(self.git("rev-parse", "HEAD^"), self.sha)
                else:
                    self.assertEqual(snapshot, self.sha)
                # Separate builders and assembly must see identical snapshots.
                self.git("checkout", "-q", "--detach", self.sha)
                subprocess.run(command, check=True, capture_output=True)
                self.assertEqual(self.git("rev-parse", "HEAD"), snapshot)
        self.assertEqual(len(keys), 3)

    def test_node_snapshot_rejects_unrelated_source_changes(self):
        plan = self.record()
        path = self.root / "crates/xmtp_sdk/Cargo.toml"
        path.write_text(path.read_text() + "\n# uncommitted source change\n")
        with self.assertRaisesRegex(ValueError, "clean source checkout"):
            version.materialize(self.root, plan, snapshot=True)
        self.assertEqual(self.git("rev-parse", "HEAD"), self.sha)

    def test_artifact_admission_rejects_corruption_extra_files_and_other_source(self):
        product = self.root / "product"
        product.mkdir()
        payload = product / "sdk.tgz"
        payload.write_bytes(b"checked payload")
        record = self.record()
        version.seal(product, record)
        version.verify(product, record)
        payload.write_bytes(b"changed payload")
        with self.assertRaisesRegex(ValueError, "checksum"):
            version.verify(product, record)
        payload.write_bytes(b"checked payload")
        (product / "unexpected").touch()
        with self.assertRaisesRegex(ValueError, "checksum"):
            version.verify(product, record)
        (product / "unexpected").unlink()
        other = record | {"sourceSha": "a" * 40}
        with self.assertRaisesRegex(ValueError, "different release"):
            version.verify(product, other)

    def test_dev_rc_and_final_have_one_valid_format(self):
        self.assertEqual(
            version.release_version("8.0.0", "dev", "1" * 40, "20261021153000"),
            "8.0.0-pre.20261021153000.dev.g1111111",
        )
        self.assertEqual(
            version.release_version("8.0.0", "rc", self.sha, "", 10), "8.0.0-rc9.1"
        )
        self.assertEqual(
            version.release_version("8.1.0", "rc", self.sha, "", 10), "8.1.0-rc.10"
        )
        self.assertEqual(
            version.release_version("8.0.0", "final", self.sha, ""), "8.0.0"
        )
        for kind, rc in [("rc", None), ("rc", 0), ("rc", -1), ("dev", 1), ("final", 1)]:
            with self.subTest(kind=kind, rc=rc), self.assertRaises(ValueError):
                version.release_version("8.0.0", kind, self.sha, "20261021153000", rc)


if __name__ == "__main__":
    unittest.main()
