"""The publication barrier and frozen artifact path are release contracts."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
SDKS = ["ios", "android", "node-sdk", "browser-sdk", "agent-sdk", "react-native-sdk"]


class ReleaseGraphTest(unittest.TestCase):
    def test_every_sdk_publish_waits_for_every_checked_product(self):
        jobs = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())[
            "jobs"
        ]
        self.assertEqual(
            set(jobs["sdk-ready"]["needs"]), {"build-" + sdk for sdk in SDKS}
        )
        self.assertEqual(
            set(jobs["sdk-complete"]["needs"]), {"release-" + sdk for sdk in SDKS}
        )
        for sdk in SDKS:
            with self.subTest(sdk=sdk):
                self.assertIn("sdk-ready", jobs["release-" + sdk]["needs"])
                self.assertIn("inputs.dry-run != true", jobs["release-" + sdk]["if"])
                self.assertTrue(jobs["build-" + sdk]["with"]["dry-run"])
                self.assertTrue(jobs["build-" + sdk]["with"]["build-only"])
                if sdk not in ("ios", "android"):
                    self.assertTrue(jobs["release-" + sdk]["with"]["prepared"])
        self.assertIn("build-node-sdk", jobs["build-agent-sdk"]["needs"])
        self.assertIn("release-node-sdk", jobs["release-agent-sdk"]["needs"])
        for sdk, entry in [("ios", "push-tag"), ("android", "publish")]:
            # Publish-only deliberately tolerates a skipped build. It must
            # still require the real permission/source preflight to succeed.
            child = yaml.safe_load(
                (ROOT / f".github/workflows/release-{sdk}.yml").read_text()
            )
            self.assertIn(
                "needs.check-push-permissions.result == 'success'",
                child["jobs"][entry]["if"],
            )
        for sdk, entries in [
            ("ios", ["publish"]),
            ("android", ["push-tag", "publish-apk"]),
        ]:
            child = yaml.safe_load(
                (ROOT / f".github/workflows/release-{sdk}.yml").read_text()
            )
            for entry in entries:
                condition = child["jobs"][entry]["if"]
                self.assertIn("!cancelled()", condition)
                for dependency in child["jobs"][entry]["needs"]:
                    self.assertIn(f"needs.{dependency}.result == 'success'", condition)

    def test_the_actual_publish_entry_refuses_missing_or_mismatched_node(self):
        # Invoke the production workflow command, with only external tools
        # stubbed. No registry or credentials are used.
        source = yaml.safe_load(
            (ROOT / ".github/workflows/npm-publish.yml").read_text()
        )
        command = next(
            step["run"]
            for step in source["jobs"]["publish"]["steps"]
            if step.get("name") == "Publish to NPM"
        )
        import io
        import tarfile

        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "dev").mkdir()
            shutil.copyfile(ROOT / "dev/publish_npm.py", root / "dev/publish_npm.py")
            shell = root / "dev/nix-shell"
            shell.write_text(
                f'#!{sys.executable}\nimport os,sys\ni=sys.argv.index("--command")+1\nos.execvp(sys.argv[i],sys.argv[i:])\n'
            )
            shell.chmod(0o755)
            npm = root / "npm"
            npm.write_text(f"""#!{sys.executable}
import json, os, sys
from pathlib import Path
args=sys.argv[1:]
with open(os.environ['CALLS'], 'a') as out: out.write(json.dumps(args)+'\\n')
if args[0]=='publish': sys.exit(0)
if args[1].startswith('@xmtp/node-sdk@'):
    value=os.environ['NODE_RESULT']
    if value!='absent':
        print(json.dumps(value)); sys.exit(0)
print(json.dumps({{'error':{{'code':'E404'}}}})); sys.exit(1)
""")
            npm.chmod(0o755)
            archive_path = root / "agent.tgz"
            with tarfile.open(archive_path, "w:gz") as archive:
                payload = json.dumps(
                    {
                        "name": "@xmtp/agent-sdk",
                        "version": "8.0.0-rc5",
                        "dependencies": {"@xmtp/node-sdk": "8.0.0-rc5"},
                    }
                ).encode()
                info = tarfile.TarInfo("package/package.json")
                info.size = len(payload)
                archive.addfile(info, io.BytesIO(payload))
            log = root / "calls.jsonl"
            for node in ("absent", "8.0.0", "8.0.0-rc5"):
                log.write_text("")
                result = subprocess.run(
                    ["bash", "-euc", command],
                    cwd=root,
                    text=True,
                    capture_output=True,
                    env=dict(
                        os.environ,
                        PATH=str(root) + os.pathsep + os.environ["PATH"],
                        PACKAGE_TARBALL=str(archive_path),
                        VERSION="8.0.0-rc5",
                        NPM_TAG="prerelease",
                        SDK="agent-sdk",
                        CALLS=str(log),
                        NODE_RESULT=node,
                    ),
                )
                calls = [json.loads(line) for line in log.read_text().splitlines()]
                self.assertEqual(
                    result.returncode, 0 if node == "8.0.0-rc5" else 1, result.stderr
                )
                self.assertEqual(
                    sum(call[0] == "publish" for call in calls),
                    1 if node == "8.0.0-rc5" else 0,
                )
                if node != "8.0.0-rc5":
                    self.assertIn("Matching Node SDK", result.stderr)


if __name__ == "__main__":
    unittest.main()
