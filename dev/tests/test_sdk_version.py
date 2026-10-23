"""The real version CLI rejects drift and materializes versions before compilation."""

import importlib.machinery
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

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
        for name in version.MANIFESTS:
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
        for name in version.MANIFESTS:
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
