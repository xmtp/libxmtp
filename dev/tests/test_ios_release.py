"""Execute the iOS publisher with real sealed products and inert GitHub transport."""

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


class IosReleaseTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.product = self.root / "target/release/ios"
        self.product.mkdir(parents=True)
        self.archive = self.product / "XmtpSdkFFI.zip"
        self.archive.write_bytes(b"retained archive bytes")
        self.record = dict(
            sourceSha="b" * 40,
            targetVersion="8.0.0",
            version="8.0.0-rc5",
            kind="rc",
            timestamp="20261030230000",
            rcNumber=5,
        )
        self.plan = self.root / "sdk-release-plan/release-plan.json"
        self.plan.parent.mkdir()
        self.plan.write_text(json.dumps(self.record))
        dev = self.root / "dev"
        dev.mkdir()
        shutil.copyfile(ROOT / "dev/sdk_version.py", dev / "sdk_version.py")
        self.write_executable(
            dev / "nix-shell",
            "#!/usr/bin/env bash\nset -euo pipefail\n"
            'if [ "$1" = --shell ]; then shift 2; fi\n'
            'if [ "$1" = --command ]; then shift; fi\nexec "$@"\n',
        )
        self.seal()
        jobs = yaml.safe_load((ROOT / ".github/workflows/release-ios.yml").read_text())[
            "jobs"
        ]
        self.shell = next(
            step["run"]
            for step in jobs["publish"]["steps"]
            if step.get("name") == "Verify and publish immutable SwiftPM archive"
        )
        self.state = self.root / "github.json"
        self.log = self.root / "github.jsonl"
        self.set_remote()
        binary = self.root / "bin"
        binary.mkdir()
        self.write_executable(
            binary / "gh",
            f"#!{sys.executable}\n"
            """import json
import os
from pathlib import Path
import subprocess
import sys

args = sys.argv[1:]
state_path = Path(os.environ['GH_TEST_STATE'])
state = json.loads(state_path.read_text())
with Path(os.environ['GH_TEST_LOG']).open('a') as log:
    log.write(json.dumps(args) + '\\n')
tag = 'libxmtp-ios-8.0.0-rc5'
if args[0] == 'api':
    if state['lookup_exit']:
        sys.exit(state['lookup_exit'])
    endpoint = next(arg for arg in args if arg.startswith('repos/'))
    if endpoint == f'repos/xmtp/libxmtp/git/matching-refs/tags/{tag}':
        assert args[args.index('--method') + 1] == 'GET'
        refs = state['neighbors'][:]
        if state['tag_sha']:
            refs.append(dict(ref='refs/tags/' + tag,
                             object=dict(sha=state['tag_sha'], type='commit')))
        payload = refs
    elif endpoint == f'repos/xmtp/libxmtp/git/ref/tags/{tag}':
        payload = dict(object=dict(sha=state['tag_sha']))
    else:
        sys.exit('Unexpected tag lookup: ' + endpoint)
    result = subprocess.run(['jq', '-r', args[args.index('--jq') + 1]],
                            input=json.dumps(payload), text=True)
    sys.exit(result.returncode)
if args[:2] == ['release', 'view']:
    sys.exit(0 if state['release_exists'] else 1)
if args[:2] == ['release', 'download']:
    assert state['release_exists']
    destination = Path(args[args.index('--dir') + 1]) / 'XmtpSdkFFI.zip'
    destination.write_bytes(state['archive'].encode())
    sys.exit(0)
if args[:2] == ['release', 'create']:
    assert not state['release_exists']
    state['release_exists'] = True
    if not state['tag_sha']:
        state['tag_sha'] = args[args.index('--target') + 1]
    state['archive'] = Path(args[3]).read_text()
    state_path.write_text(json.dumps(state))
    sys.exit(0)
sys.exit('Unexpected GitHub operation: ' + str(args))
""",
        )
        self.env = dict(
            os.environ,
            PATH=str(binary) + os.pathsep + os.environ["PATH"],
            RUNNER_TEMP=str(self.root),
            VERSION=self.record["version"],
            SOURCE_SHA=self.record["sourceSha"],
            GITHUB_REPOSITORY="xmtp/libxmtp",
            GH_TEST_STATE=str(self.state),
            GH_TEST_LOG=str(self.log),
        )

    def write_executable(self, path, text):
        path.write_text(text)
        path.chmod(0o755)

    def seal(self):
        subprocess.run(
            [
                sys.executable,
                str(self.root / "dev/sdk_version.py"),
                "seal",
                str(self.product),
                "--plan",
                str(self.plan),
            ],
            check=True,
            capture_output=True,
            text=True,
        )

    def set_remote(self, **overrides):
        state = dict(
            tag_sha=None,
            release_exists=False,
            lookup_exit=0,
            neighbors=[],
            archive="retained archive bytes",
        )
        self.state.write_text(json.dumps(dict(state, **overrides)))

    def publish(self, **env):
        self.log.unlink(missing_ok=True)
        result = subprocess.run(
            ["bash", "-euc", self.shell],
            cwd=self.root,
            env=dict(self.env, **env),
            capture_output=True,
            text=True,
        )
        calls = (
            [json.loads(line) for line in self.log.read_text().splitlines()]
            if self.log.exists()
            else []
        )
        return result, calls

    def test_requested_identity_must_match_the_frozen_plan_before_transport(self):
        for override in (dict(VERSION="8.0.0-rc6"), dict(SOURCE_SHA="a" * 40)):
            with self.subTest(override=override):
                self.set_remote()
                result, calls = self.publish(**override)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual(calls, [])

    def test_product_identity_must_match_the_plan_before_transport(self):
        for field, value in (("version", "8.0.0-rc6"), ("sourceSha", "a" * 40)):
            with self.subTest(field=field):
                receipt = self.product / "release-product.json"
                data = json.loads(receipt.read_text())
                data["release"] = dict(self.record, **{field: value})
                receipt.write_text(json.dumps(data))
                result, calls = self.publish()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])

    def test_corrupt_retained_bytes_fail_before_transport(self):
        self.archive.write_bytes(b"changed after sealing")
        result, calls = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checksum mismatch", result.stderr)
        self.assertEqual(calls, [])

    def test_wrong_source_tag_rejects_orphans_and_existing_releases_on_retries(self):
        for release_exists in (False, True):
            self.set_remote(tag_sha="a" * 40, release_exists=release_exists)
            for attempt in range(2):
                with self.subTest(release_exists=release_exists, attempt=attempt):
                    result, calls = self.publish()
                    self.assertNotEqual(result.returncode, 0)
                    self.assertTrue(calls)
                    self.assertTrue(all(call[0] == "api" for call in calls), calls)

    def test_correct_source_orphan_is_resumable_and_retry_verifies_bytes(self):
        self.set_remote(tag_sha=self.record["sourceSha"])
        result, calls = self.publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sum(call[:2] == ["release", "create"] for call in calls), 1)
        result, calls = self.publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(call[:2] == ["release", "download"] for call in calls))
        self.assertFalse(any(call[:2] == ["release", "create"] for call in calls))

    def test_absent_exact_tag_ignores_prefix_neighbors(self):
        for neighbors in (
            [],
            [
                dict(
                    ref="refs/tags/libxmtp-ios-8.0.0-rc50",
                    object=dict(sha="a" * 40, type="commit"),
                )
            ],
        ):
            with self.subTest(neighbors=neighbors):
                self.set_remote(neighbors=neighbors)
                result, calls = self.publish()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(calls[0][0], "api")
                create = next(
                    call for call in calls if call[:2] == ["release", "create"]
                )
                self.assertEqual(create[2], "libxmtp-ios-" + self.record["version"])
                self.assertEqual(
                    create[create.index("--target") + 1], self.record["sourceSha"]
                )

    def test_tag_lookup_errors_fail_closed_before_release_transport(self):
        for status in (1, 4, 22):
            with self.subTest(status=status):
                self.set_remote(lookup_exit=status)
                result, calls = self.publish()
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(calls)
                self.assertTrue(all(call[0] == "api" for call in calls), calls)

    def test_existing_archive_conflict_is_never_overwritten(self):
        self.set_remote(
            tag_sha=self.record["sourceSha"],
            release_exists=True,
            archive="different public archive bytes",
        )
        result, calls = self.publish()
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(any(call[:2] == ["release", "download"] for call in calls))
        self.assertFalse(any(call[:2] == ["release", "create"] for call in calls))


if __name__ == "__main__":
    unittest.main()
