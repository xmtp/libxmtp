#!/usr/bin/env python3
"""Check isolated release tag transfer with a local Git HTTP remote."""

import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import re
import subprocess
import tempfile
from textwrap import dedent
import threading
import unittest
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/push-release-tag.yml"


def script(name):
    text = WORKFLOW.read_text().split("      - name: " + name + "\n", 1)[1]
    text = text.split("      - ", 1)[0]
    return dedent(text.split("        run: |\n", 1)[1])


class ReleasePushTest(unittest.TestCase):
    def test_bundle_push_retains_commit_without_persisting_token(self):
        for sdk in ["ios", "android"]:
            with self.subTest(sdk=sdk):
                self.bundle_push(sdk)

    def bundle_push(self, sdk):
        requests = []
        with tempfile.TemporaryDirectory(prefix="release-push-") as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            temporary = root / "runner"
            (temporary / "release-tag").mkdir(parents=True)
            env = {
                key: value
                for key, value in os.environ.items()
                if not key.startswith("GIT_")
            }
            env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)

            def git(*args, cwd=source):
                return subprocess.run(
                    ["git", *args],
                    cwd=cwd,
                    env=env,
                    check=True,
                    capture_output=True,
                    text=True,
                ).stdout

            git("init", "--initial-branch=main")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.com")
            git("-c", "commit.gpgSign=false", "commit", "--allow-empty", "-m", "source")
            source_sha = git("rev-parse", "HEAD").strip()
            if sdk == "ios":
                (source / "Package.swift").write_text("generated release metadata")
                git("add", "Package.swift")
                git("-c", "commit.gpgSign=false", "commit", "-m", "release")
            expected = git("rev-parse", "HEAD").strip()
            git("-c", "tag.gpgSign=false", "tag", f"{sdk}-1.2.3")
            hook = source / ".git/hooks/pre-push"
            hook.write_text("#!/bin/sh\nexit 99\n")
            hook.chmod(0o755)
            git(
                "bundle",
                "create",
                str(temporary / "release-tag/release.bundle"),
                f"refs/tags/{sdk}-1.2.3",
            )
            remote = root / "fixture/repo.git"
            git("init", "--bare", str(remote))
            git("--git-dir=" + str(remote), "config", "http.receivepack", "true")
            backend = Path(git("--exec-path").strip()) / "git-http-backend"

            class Handler(BaseHTTPRequestHandler):
                def handle_git(self):
                    requests.append(self.headers.get_all("Authorization", []))
                    url = urlsplit(self.path)
                    body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
                    result = subprocess.run(
                        [str(backend)],
                        input=body,
                        capture_output=True,
                        check=True,
                        env=dict(
                            env,
                            GIT_PROJECT_ROOT=str(root),
                            GIT_HTTP_EXPORT_ALL="1",
                            REQUEST_METHOD=self.command,
                            PATH_INFO=url.path,
                            QUERY_STRING=url.query,
                            CONTENT_TYPE=self.headers.get("Content-Type", ""),
                            CONTENT_LENGTH=str(len(body)),
                        ),
                    )
                    headers, response = result.stdout.split(b"\r\n\r\n", 1)
                    self.send_response(200)
                    for line in headers.decode().splitlines():
                        key, value = line.split(":", 1)
                        self.send_header(key, value.strip())
                    self.end_headers()
                    self.wfile.write(response)

                do_GET = handle_git
                do_POST = handle_git

                def log_message(self, *_args):
                    pass

            server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            worker = threading.Thread(target=server.serve_forever, daemon=True)
            worker.start()
            try:
                token = "fixture-release-token"
                step_env = dict(
                    env,
                    RUNNER_TEMP=str(temporary),
                    SDK=sdk,
                    VERSION="1.2.3",
                    SOURCE_SHA=source_sha,
                    GITHUB_SERVER_URL=f"http://127.0.0.1:{server.server_port}",
                    GITHUB_REPOSITORY="fixture/repo",
                    RELEASE_TOKEN=token,
                )
                subprocess.run(
                    ["bash", "-euc", script("Import release tag")],
                    cwd=root,
                    env=step_env,
                    check=True,
                    capture_output=True,
                )
                result = subprocess.run(
                    ["bash", "-euc", script("Push release tag")],
                    cwd=root,
                    env=step_env,
                    check=True,
                    capture_output=True,
                    text=True,
                )
                encoded = base64.b64encode(f"x-access-token:{token}".encode()).decode()
                self.assertEqual(
                    result.stdout.strip(), f"::add-mask::AUTHORIZATION: basic {encoded}"
                )
                self.assertEqual(
                    expected,
                    git(
                        "--git-dir=" + str(remote), "rev-parse", f"{sdk}-1.2.3"
                    ).strip(),
                )
                self.assertTrue(requests)
                self.assertTrue(
                    all(headers == [f"basic {encoded}"] for headers in requests),
                    requests,
                )
                config = (temporary / "release-push.git/config").read_text()
                self.assertNotIn(token, config)
                self.assertNotIn(encoded, config)
                self.assertNotIn("extraheader", config)
                self.assertNotIn("credential", config)
            finally:
                server.shutdown()
                worker.join()
                server.server_close()

            # A different expected tag must fail before the token is minted.
            other = root / "other-runner"
            (other / "release-tag").mkdir(parents=True)
            (other / "release-tag/release.bundle").write_bytes(
                (temporary / "release-tag/release.bundle").read_bytes()
            )
            result = subprocess.run(
                ["bash", "-euc", script("Import release tag")],
                cwd=root,
                env=dict(step_env, RUNNER_TEMP=str(other), VERSION="1.2.4"),
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)

    def test_rejects_a_release_commit_that_changes_workflows(self):
        for sdk in ["ios", "android"]:
            with self.subTest(sdk=sdk), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                temporary = root / "runner"
                (temporary / "release-tag").mkdir(parents=True)
                env = {
                    key: value
                    for key, value in os.environ.items()
                    if not key.startswith("GIT_")
                }
                env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)

                def git(*args):
                    return subprocess.run(
                        ["git", *args],
                        cwd=root,
                        env=env,
                        check=True,
                        capture_output=True,
                        text=True,
                    ).stdout.strip()

                git("init", "--initial-branch=main")
                git("config", "user.name", "Test")
                git("config", "user.email", "test@example.com")
                git(
                    "-c",
                    "commit.gpgSign=false",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "source",
                )
                source_sha = git("rev-parse", "HEAD")
                (root / ".github/workflows").mkdir(parents=True)
                (root / ".github/workflows/injected.yml").write_text(
                    "name: injected workflow\n"
                )
                git("add", ".github/workflows/injected.yml")
                git("-c", "commit.gpgSign=false", "commit", "-m", "release")
                tag = f"refs/tags/{sdk}-1.2.3"
                git("-c", "tag.gpgSign=false", "tag", f"{sdk}-1.2.3")
                git(
                    "bundle",
                    "create",
                    str(temporary / "release-tag/release.bundle"),
                    tag,
                )
                result = subprocess.run(
                    ["bash", "-euc", script("Import release tag")],
                    cwd=root,
                    env=dict(
                        env,
                        RUNNER_TEMP=str(temporary),
                        SDK=sdk,
                        VERSION="1.2.3",
                        SOURCE_SHA=source_sha,
                    ),
                    capture_output=True,
                )
                self.assertNotEqual(result.returncode, 0)

    def test_app_token_is_confined_to_separate_push_job(self):
        text = WORKFLOW.read_text()
        self.assertIn("permissions: {}", text)
        self.assertNotIn("checkout@", text)
        self.assertNotIn("xmtp-release", text)
        self.assertNotIn("uses: ./", text)
        self.assertLess(
            text.index("- name: Import release tag"),
            text.index("- name: Create release push token"),
        )
        self.assertRegex(
            text, r"uses: actions/create-github-app-token@[0-9a-f]{40}(?:\s|$)"
        )
        self.assertIn("permission-contents: write", text)
        self.assertIn("permission-workflows: write", text)
        self.assertNotRegex(text, re.compile(r"^\s+owner:", re.MULTILINE))
        preflight = (ROOT / ".github/workflows/check-release-push.yml").read_text()
        self.assertNotIn("checkout@", preflight)
        self.assertNotIn("run:", preflight)
        self.assertIn("permission-contents: write", preflight)
        self.assertIn("permission-workflows: write", preflight)
        for sdk in ["android", "ios"]:
            with self.subTest(sdk=sdk):
                mobile = (ROOT / f".github/workflows/release-{sdk}.yml").read_text()
                self.assertNotIn("GH_APP_PK", mobile)
                self.assertIn("--no-push", mobile)
                self.assertIn("fetch-depth: 0", mobile)
                self.assertIn(f"name: release-tag-{sdk}", mobile)
                self.assertIn("uses: ./.github/workflows/push-release-tag.yml", mobile)
                self.assertIn("source-sha: ${{ github.sha }}", mobile)
                self.assertIn(
                    "uses: ./.github/workflows/check-release-push.yml", mobile
                )
                self.assertRegex(
                    mobile, r"needs: \[[^\n]*check-push-permissions[^\n]*\]"
                )
        ios = (ROOT / ".github/workflows/release-ios.yml").read_text()
        self.assertIn("needs: [prepare-release, push-tag]", ios)
        self.assertIn("name: ios-release-bundle", ios)


if __name__ == "__main__":
    unittest.main()
