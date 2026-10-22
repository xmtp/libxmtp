#!/usr/bin/env python3
"""Exercise publication retries against local registries that reject overwrites."""

import importlib.util
import io
import json
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import tempfile
import threading
import unittest
import urllib.parse
import uuid
import xml.etree.ElementTree as ET
import zipfile

import yaml

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "android_publish", ROOT / "sdks/android/dev/publish.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class AndroidPublishTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.repo = self.root / "repo"
        self.version = "8.0.0-rc1"
        self.files = self.repo / "org/xmtp/android" / self.version
        self.files.mkdir(parents=True)
        for suffix in (".aar", ".pom", "-sources.jar", "-javadoc.jar"):
            name = f"android-{self.version}{suffix}"
            (self.files / name).write_bytes(name.encode())
            (self.files / (name + ".asc")).write_bytes(b"signature")
        self.remote = {}
        self.puts = []
        self.deployments = {}
        self.releases = []
        self.lose_put_response = False
        self.lose_release_response = False
        self.lose_upload_response = False
        self.github_forbidden = False
        test = self

        class Registry(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def respond(self, status, body=b""):
                self.send_response(status)
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                if test.github_forbidden:
                    return self.respond(403, b"secret error body")
                test.assertEqual(self.headers["Authorization"], "Basic github-token")
                self.respond(
                    200, test.remote[self.path]
                ) if self.path in test.remote else self.respond(404)

            def do_PUT(self):
                test.assertEqual(self.headers["Authorization"], "Basic github-token")
                metadata = "/maven-metadata.xml" in self.path
                if self.path in test.remote and not metadata:
                    return self.respond(409)
                if not metadata:
                    test.puts.append(self.path)
                test.remote[self.path] = self.rfile.read(
                    int(self.headers["Content-Length"])
                )
                if test.lose_put_response:
                    test.lose_put_response = False
                    return self.respond(500)
                self.respond(201)

            def do_POST(self):
                test.assertEqual(self.headers["Authorization"], "Bearer central-token")
                path = urllib.parse.urlsplit(self.path)
                if path.path == "/central/upload":
                    query = urllib.parse.parse_qs(path.query)
                    test.assertEqual(query["publishingType"], ["USER_MANAGED"])
                    body = self.rfile.read(int(self.headers["Content-Length"]))
                    boundary = (
                        self.headers["Content-Type"].split("boundary=")[1].encode()
                    )
                    bundle = body.split(b"\r\n\r\n", 1)[1].rsplit(
                        b"\r\n--" + boundary, 1
                    )[0]
                    with zipfile.ZipFile(io.BytesIO(bundle)) as archive:
                        test.assertEqual(len(archive.namelist()), 8)
                        for name in archive.namelist():
                            test.assertEqual(
                                archive.read(name), (test.repo / name).read_bytes()
                            )
                    deployment = str(uuid.uuid4())
                    test.deployments[deployment] = "VALIDATED"
                    if test.lose_upload_response:
                        test.lose_upload_response = False
                        return self.respond(500)
                    return self.respond(201, deployment.encode())
                if path.path == "/central/status":
                    deployment = urllib.parse.parse_qs(path.query)["id"][0]
                    return self.respond(
                        200,
                        json.dumps(
                            {"deploymentState": test.deployments[deployment]}
                        ).encode(),
                    )
                if path.path.startswith("/central/deployment/"):
                    deployment = path.path.rsplit("/", 1)[1]
                    test.assertEqual(test.deployments[deployment], "VALIDATED")
                    test.releases.append(deployment)
                    test.deployments[deployment] = "PUBLISHED"
                    if test.lose_release_response:
                        test.lose_release_response = False
                        return self.respond(500)
                    return self.respond(204)
                self.respond(404)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Registry)
        worker = threading.Thread(target=self.server.serve_forever, daemon=True)
        worker.start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)

    def publisher(self):
        publisher = MODULE.Publisher(
            self.repo,
            self.root / "state",
            self.version,
            "source-sha",
            "Basic github-token",
            "Bearer central-token",
        )
        url = f"http://127.0.0.1:{self.server.server_port}"
        publisher.github_url = url + "/github"
        publisher.central_url = url + "/central"
        return publisher

    def test_partial_github_upload_resumes_without_overwrite_or_early_central(self):
        self.lose_put_response = True
        with self.assertRaisesRegex(RuntimeError, "HTTP 500"):
            self.publisher().stage()
        self.assertEqual(len(self.remote), 1)
        self.assertEqual(self.deployments, {})
        self.assertEqual(self.releases, [])
        self.publisher().stage()
        self.assertEqual(len(self.puts), 8)
        self.assertEqual(len(set(self.puts)), 8)
        self.assertEqual(len(self.deployments), 1)
        self.assertEqual(self.releases, [])
        self.publisher().release(interval=0)
        self.assertEqual(len(self.releases), 1)

    def test_lost_central_release_response_resumes_same_deployment(self):
        self.publisher().stage()
        self.lose_release_response = True
        with self.assertRaisesRegex(RuntimeError, "HTTP 500"):
            self.publisher().release(interval=0)
        self.publisher().stage()
        self.publisher().release(interval=0)
        self.assertEqual(len(self.deployments), 1)
        self.assertEqual(len(self.releases), 1)
        self.assertEqual(len(self.puts), 8)

    def test_maven_version_index_preserves_other_releases_on_retry(self):
        path = "/github/org/xmtp/android/maven-metadata.xml"
        self.remote[path] = (
            b"<metadata><groupId>org.xmtp</groupId><artifactId>android</artifactId><versioning><versions><version>7.0.0</version></versions></versioning></metadata>"
        )
        self.publisher().stage()
        self.publisher().stage()
        metadata = ET.fromstring(self.remote[path])
        self.assertEqual(
            [v.text for v in metadata.findall("versioning/versions/version")],
            ["7.0.0", self.version],
        )
        self.assertEqual(len(self.deployments), 1)

    def test_lost_upload_response_cannot_publish_an_unrecorded_deployment(self):
        self.lose_upload_response = True
        with self.assertRaisesRegex(RuntimeError, "HTTP 500"):
            self.publisher().stage()
        with self.assertRaisesRegex(RuntimeError, "must be saved"):
            self.publisher().release(interval=0)
        self.publisher().stage()
        self.publisher().release(interval=0)
        self.assertEqual(len(self.deployments), 2)
        self.assertEqual(list(self.deployments.values()).count("PUBLISHED"), 1)

    def test_different_remote_bytes_and_auth_failures_block_central(self):
        first = sorted(self.files.iterdir())[0]
        self.remote[f"/github/org/xmtp/android/{self.version}/{first.name}"] = (
            b"other-release"
        )
        with self.assertRaisesRegex(RuntimeError, "different bytes"):
            self.publisher().stage()
        self.assertEqual(self.puts, [])
        self.assertEqual(self.deployments, {})
        self.github_forbidden = True
        with self.assertRaisesRegex(RuntimeError, "HTTP 403") as caught:
            self.publisher().stage()
        self.assertNotIn("secret", str(caught.exception))
        self.assertEqual(self.deployments, {})

    def test_checkpoint_rejects_changed_bundle_and_source(self):
        self.publisher().stage()
        artifact = sorted(self.files.iterdir())[0]
        original = artifact.read_bytes()
        artifact.write_bytes(b"different")
        with self.assertRaisesRegex(RuntimeError, "different release bytes or source"):
            self.publisher()
        artifact.write_bytes(original)
        state = self.root / "state/state.json"
        data = json.loads(state.read_text())
        data["source"] = "other-source"
        state.write_text(json.dumps(data))
        with self.assertRaisesRegex(RuntimeError, "different release bytes or source"):
            self.publisher()

    def test_failed_or_pending_validation_never_releases(self):
        self.publisher().stage()
        deployment = next(iter(self.deployments))
        for state in ("FAILED", "PENDING"):
            self.deployments[deployment] = state
            with self.assertRaises(RuntimeError):
                self.publisher().release(timeout=0, interval=0)
            self.assertEqual(self.releases, [])

    def test_workflow_saves_deployment_before_finalizing_and_gates_tag(self):
        jobs = yaml.safe_load(
            (ROOT / ".github/workflows/release-android.yml").read_text()
        )["jobs"]
        steps = jobs["publish-packages"]["steps"]
        names = [step.get("name") for step in steps]
        self.assertLess(
            names.index("Upload GitHub Packages and stage Central"),
            names.index("Retain Central deployment"),
        )
        self.assertLess(
            names.index("Retain Central deployment"), names.index("Finalize Central")
        )
        checkpoint = steps[names.index("Retain Central deployment")]
        self.assertNotIn("continue-on-error", checkpoint)
        self.assertNotIn("if", steps[names.index("Finalize Central")])
        self.assertIn("publish-packages", jobs["push-tag"]["needs"])
        self.assertIn("publish", jobs["publish-packages"]["needs"])


if __name__ == "__main__":
    unittest.main()
