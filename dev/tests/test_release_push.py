#!/usr/bin/env python3
"""Check release push credentials with a local Git HTTP remote."""

import base64
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import re
import subprocess
import tempfile
from textwrap import dedent
import threading
import unittest

ROOT = Path(__file__).resolve().parents[2]
ACTION = ROOT / ".github/actions/setup-release-push/action.yml"


class ReleasePushTest(unittest.TestCase):
    def test_release_token_replaces_checkout_credentials(self):
        action = ACTION.read_text()
        script = dedent(action.split("      run: |\n", 1)[1])
        requests = []

        class Handler(SimpleHTTPRequestHandler):
            def do_GET(self):
                requests.append(self.headers.get_all("Authorization", []))
                super().do_GET()

            def log_message(self, *_args):
                pass

        with tempfile.TemporaryDirectory(prefix="release-push-") as directory:
            root = Path(directory)
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
                ).stdout

            git("init", "--initial-branch=main")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.com")
            git("-c", "commit.gpgSign=false", "commit", "--allow-empty", "-m", "test")
            git("clone", "--bare", ".", "remote.git")
            git("--git-dir=remote.git", "update-server-info")
            server = ThreadingHTTPServer(
                ("127.0.0.1", 0), partial(Handler, directory=str(root))
            )
            worker = threading.Thread(target=server.serve_forever, daemon=True)
            worker.start()
            try:
                url = f"http://127.0.0.1:{server.server_port}"
                key = f"http.{url}/.extraheader"
                # Checkout v6 can put its credential in an included file.
                auth_file = root / ".git/checkout-auth"
                git("config", "--file", str(auth_file), key, "AUTHORIZATION: basic old")
                git("config", "include.path", str(auth_file))
                token = "fixture-release-token"
                result = subprocess.run(
                    ["bash", "-euc", script],
                    cwd=root,
                    env=dict(env, RELEASE_TOKEN=token, GITHUB_SERVER_URL=url),
                    check=True,
                    capture_output=True,
                    text=True,
                )
                encoded = base64.b64encode(f"x-access-token:{token}".encode()).decode()
                self.assertEqual(
                    result.stdout.strip(), f"::add-mask::AUTHORIZATION: basic {encoded}"
                )
                self.assertIn("refs/heads/main", git("ls-remote", f"{url}/remote.git"))
                self.assertTrue(requests)
                self.assertTrue(
                    all(headers == [f"basic {encoded}"] for headers in requests),
                    requests,
                )
            finally:
                server.shutdown()
                worker.join()
                server.server_close()

    def test_mobile_jobs_request_push_scopes_before_publication(self):
        action = ACTION.read_text()
        self.assertRegex(
            action, r"uses: actions/create-github-app-token@[0-9a-f]{40}(?:\s|$)"
        )
        self.assertIn("permission-contents: write", action)
        self.assertIn("permission-workflows: write", action)
        # No owner input keeps the token scoped to the current repository.
        self.assertNotRegex(action, re.compile(r"^\s+owner:", re.MULTILINE))
        for sdk, publication in [
            ("android", "Publish"),
            ("ios", "Update Package.swift and podspec"),
        ]:
            with self.subTest(sdk=sdk):
                text = (ROOT / f".github/workflows/release-{sdk}.yml").read_text()
                setup = text.split("      - name: Set up release push\n", 1)[1]
                setup = setup.split("      - ", 1)[0]
                self.assertIn("uses: ./.github/actions/setup-release-push", setup)
                self.assertIn("app-id: ${{ secrets.GH_APP_ID }}", setup)
                self.assertIn("private-key: ${{ secrets.GH_APP_PK }}", setup)
                self.assertNotIn("if:", setup)
                self.assertNotIn("continue-on-error", setup)
                self.assertLess(
                    text.index("- name: Set up release push"),
                    text.index(f"- name: {publication}\n"),
                )


if __name__ == "__main__":
    unittest.main()
