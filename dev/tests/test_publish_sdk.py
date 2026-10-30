"""Release retry/admission behavior with registry commands stubbed, never published."""

import base64
from contextlib import redirect_stdout
import hashlib
import importlib.machinery
import importlib.util
import io
import os
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader(
    "publish_npm", str(ROOT / "dev/publish_npm.py")
)
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)


class PublishNpmTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.tarball = Path(self.temp.name) / "package.tgz"

    def pack(self, name="@xmtp/node-sdk", version="8.0.0-rc5", node="8.0.0-rc5"):
        payload = json.dumps(
            dict(name=name, version=version, dependencies={"@xmtp/node-sdk": node})
        ).encode()
        with tarfile.open(self.tarball, "w:gz") as archive:
            info = tarfile.TarInfo("package/package.json")
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))
        return (
            "sha512-"
            + base64.b64encode(
                hashlib.sha512(self.tarball.read_bytes()).digest()
            ).decode()
        )

    def test_matching_retry_skips_publish_and_conflicting_bytes_fail(self):
        integrity = self.pack()
        for existing in (integrity, "sha512-other"):
            with (
                self.subTest(existing=existing),
                patch.object(module.subprocess, "run") as run,
            ):
                run.return_value = subprocess.CompletedProcess(
                    [], 0, json.dumps(existing), ""
                )
                if existing == integrity:
                    module.publish(self.tarball, "8.0.0-rc5", "prerelease", "node-sdk")
                else:
                    with self.assertRaisesRegex(ValueError, "different bytes"):
                        module.publish(
                            self.tarball, "8.0.0-rc5", "prerelease", "node-sdk"
                        )
                self.assertEqual(run.call_count, 1)

    def test_only_registry_absence_allows_publication(self):
        self.pack()
        for code in ("E404", "E401", "E500"):
            with self.subTest(code=code), patch.object(module.subprocess, "run") as run:
                run.return_value = subprocess.CompletedProcess(
                    [], 1, json.dumps({"error": {"code": code}}), ""
                )
                if code == "E404":
                    module.publish(self.tarball, "8.0.0-rc5", "prerelease", "node-sdk")
                    self.assertEqual(
                        run.call_args.args[0],
                        [
                            "npm",
                            "publish",
                            str(self.tarball),
                            "--provenance",
                            "--ignore-scripts",
                            "--tag",
                            "prerelease",
                        ],
                    )
                    self.assertTrue(run.call_args.kwargs["check"])
                else:
                    with self.assertRaisesRegex(ValueError, "lookup failed"):
                        module.publish(
                            self.tarball, "8.0.0-rc5", "prerelease", "node-sdk"
                        )
                    self.assertEqual(run.call_count, 1)

    def test_package_identity_version_and_node_pin_are_admission_guards(self):
        for name, version, node, expected in [
            ("@xmtp/browser-sdk", "8.0.0-rc5", "8.0.0-rc5", "Package name"),
            ("@xmtp/agent-sdk", "8.0.0", "8.0.0-rc5", "Package version"),
            ("@xmtp/agent-sdk", "8.0.0-rc5", "latest", "matching Node"),
            ("@xmtp/cli", "0.3.0", "8.0.0-rc5", "Package version"),
            ("@xmtp/cli", "8.0.0-rc5", "latest", "matching Node"),
        ]:
            with (
                self.subTest(name=name, version=version, node=node),
                patch.object(module.subprocess, "run") as run,
            ):
                integrity = self.pack(name, version, node)
                run.return_value = subprocess.CompletedProcess(
                    [], 0, json.dumps(integrity), ""
                )
                sdk = (
                    name.removeprefix("@xmtp/")
                    if name in ("@xmtp/agent-sdk", "@xmtp/cli")
                    else "node-sdk"
                )
                with self.assertRaisesRegex(ValueError, expected):
                    module.publish(self.tarball, "8.0.0-rc5", "prerelease", sdk)
                run.assert_not_called()


# The Maven publication uses Gradle's existing Sonatype transport. Its admission
# check compares the actual prepared payloads before that transport is invoked.
loader = importlib.machinery.SourceFileLoader(
    "check_maven", str(ROOT / "dev/check_maven_release.py")
)
spec = importlib.util.spec_from_loader(loader.name, loader)
maven = importlib.util.module_from_spec(spec)
loader.exec_module(maven)


class MavenAdmissionTest(unittest.TestCase):
    def test_registry_redirects_keep_authorization_only_on_the_same_origin(self):
        source = "https://maven.pkg.github.com/xmtp/libxmtp/asset"
        for destination, expected in [
            ("https://maven.pkg.github.com/next", "Basic secret"),
            ("https://maven.pkg.github.com:443/next", "Basic secret"),
            ("https://storage.example/next", None),
            ("http://maven.pkg.github.com/next", None),
            ("https://maven.pkg.github.com:444/next", None),
        ]:
            with self.subTest(destination=destination):
                request = maven.urllib.request.Request(
                    source, headers={"Authorization": "Basic secret"}
                )
                redirected = maven.RegistryRedirects().redirect_request(
                    request, None, 302, "Found", {}, destination
                )
                self.assertEqual(redirected.get_header("Authorization"), expected)

    def test_absent_identical_conflicting_and_partially_visible_publications(self):
        import urllib.error

        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder)
            for suffix in (".aar", ".pom", "-sources.jar", "-javadoc.jar"):
                (directory / ("android-8.0.0-rc5" + suffix)).write_bytes(
                    suffix.encode()
                )
            for repository, host in [
                ("central", "https://repo.maven.apache.org/maven2"),
                ("github", "https://maven.pkg.github.com/xmtp/libxmtp"),
            ]:
                for case in (
                    "absent",
                    "identical",
                    "conflict",
                    "partial",
                    "unauthorized",
                ):
                    with self.subTest(repository=repository, case=case):

                        def response(request, timeout):
                            url = request.full_url
                            self.assertEqual(
                                request.get_header("Authorization"),
                                "Basic YWN0b3I6dG9rZW4="
                                if repository == "github"
                                else None,
                            )
                            self.assertEqual(timeout, 60)
                            self.assertEqual(
                                url.rsplit("/", 1)[0],
                                host + "/org/xmtp/android/8.0.0-rc5",
                            )
                            payload = (directory / url.rsplit("/", 1)[1]).read_bytes()
                            if case == "unauthorized":
                                raise urllib.error.HTTPError(
                                    url, 401, "Denied", {}, None
                                )
                            if case == "absent" or (
                                case == "partial" and url.endswith(".pom")
                            ):
                                raise urllib.error.HTTPError(
                                    url, 404, "Absent", {}, None
                                )
                            return io.BytesIO(
                                b"conflict" if case == "conflict" else payload
                            )

                        with (
                            patch.object(maven.OPENER, "open", side_effect=response),
                            patch.dict(
                                os.environ, GITHUB_ACTOR="actor", GITHUB_TOKEN="token"
                            ),
                        ):
                            if case == "unauthorized":
                                with self.assertRaises(urllib.error.HTTPError):
                                    maven.check(directory, "8.0.0-rc5", repository)
                            elif case in ("conflict", "partial"):
                                with self.assertRaisesRegex(
                                    ValueError,
                                    "different bytes"
                                    if case == "conflict"
                                    else "incomplete",
                                ):
                                    maven.check(directory, "8.0.0-rc5", repository)
                            else:
                                output = io.StringIO()
                                with redirect_stdout(output):
                                    maven.check(directory, "8.0.0-rc5", repository)
                                self.assertEqual(
                                    output.getvalue(),
                                    "absent\n" if case == "absent" else "existing\n",
                                )


if __name__ == "__main__":
    unittest.main()
