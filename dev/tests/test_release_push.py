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


def script(name, workflow=WORKFLOW):
    text = workflow.read_text().split("      - name: " + name + "\n", 1)[1]
    text = text.split("      - ", 1)[0]
    return dedent(text.split("        run: |\n", 1)[1])


class ReleasePushTest(unittest.TestCase):
    def test_selected_source_is_fixed_before_sdk_commands(self):
        with tempfile.TemporaryDirectory() as directory:
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
                ).stdout.strip()

            git("init", "--initial-branch=caller")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.com")
            git("-c", "commit.gpgSign=false", "commit", "--allow-empty", "-m", "caller")
            caller_sha = git("rev-parse", "HEAD")
            git("switch", "-c", "release")
            git(
                "-c",
                "commit.gpgSign=false",
                "commit",
                "--allow-empty",
                "-m",
                "selected",
            )
            selected_sha = git("rev-parse", "HEAD")
            output = root / "source-output"
            subprocess.run(
                [
                    "bash",
                    "-euc",
                    script(
                        "Resolve release source",
                        ROOT / ".github/workflows/check-release-push.yml",
                    ),
                ],
                cwd=root,
                env=dict(env, GITHUB_SHA=caller_sha, GITHUB_OUTPUT=str(output)),
                check=True,
            )
            snapshot = output.read_text().strip().removeprefix("sha=")
            self.assertEqual(snapshot, selected_sha)
            self.assertNotEqual(snapshot, caller_sha)
            # A later branch update must not change the saved source commit.
            git("-c", "commit.gpgSign=false", "commit", "--allow-empty", "-m", "later")
            self.assertNotEqual(git("rev-parse", "release"), snapshot)
            git("checkout", "--detach", snapshot)
            git("-c", "tag.gpgSign=false", "tag", "android-1.2.3")
            temporary = root / "runner"
            (temporary / "release-tag").mkdir(parents=True)
            git(
                "bundle",
                "create",
                str(temporary / "release-tag/release.bundle"),
                "refs/tags/android-1.2.3",
            )
            subprocess.run(
                ["bash", "-euc", script("Import release tag")],
                cwd=root,
                env=dict(
                    env,
                    SOURCE_SHA=snapshot,
                    SDK="android",
                    VERSION="1.2.3",
                    RUNNER_TEMP=str(temporary),
                ),
                check=True,
                capture_output=True,
            )

    def test_bundle_push_retains_commit_without_persisting_token(self):
        for sdk in ["ios", "android", "node-sdk", "browser-sdk", "agent-sdk", "cli"]:
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
        for sdk in ["ios", "android", "node-sdk", "browser-sdk", "agent-sdk", "cli"]:
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
        self.assertRegex(preflight, r"uses: actions/checkout@[0-9a-f]{40}(?:\s|$)")
        self.assertIn("persist-credentials: false", preflight)
        self.assertIn("ref: ${{ inputs.ref }}", preflight)
        self.assertNotIn("uses: ./", preflight)
        self.assertNotIn("xmtp-release", preflight)
        self.assertLess(
            preflight.index("- name: Resolve release source"),
            preflight.index("- name: Verify release App permissions"),
        )
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
                source = "${{ needs.check-push-permissions.outputs.source-sha }}"
                self.assertIn("source-sha: " + source, mobile)
                self.assertNotIn("source-sha: ${{ github.sha }}", mobile)
                self.assertIn("REF: " + source, mobile)
                self.assertIn("ref: " + source, mobile)
                for body in re.split(
                    r"(?m)^  [a-z][a-z0-9-]*:\n", mobile.split("jobs:\n", 1)[1]
                ):
                    if source in body:
                        self.assertRegex(
                            body, r"needs: \[[^\n]*check-push-permissions[^\n]*\]"
                        )
                self.assertIn(
                    "uses: ./.github/workflows/check-release-push.yml", mobile
                )
                self.assertRegex(
                    mobile, r"needs: \[[^\n]*check-push-permissions[^\n]*\]"
                )
        ios = (ROOT / ".github/workflows/release-ios.yml").read_text()
        self.assertIn("needs: [prepare-release, push-tag, check-push-permissions]", ios)
        self.assertIn("name: ios-release-bundle", ios)

    def test_npm_source_job_only_creates_local_tags(self):
        npm = ROOT / ".github/workflows/npm-publish.yml"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "bin"
            binary.mkdir()
            runner = root / "runner"
            runner.mkdir()
            tools = ROOT / "dev/release-tools"
            wrapper = binary / "xmtp-release"
            wrapper.write_text(
                '#!/usr/bin/env bash\nexec "'
                + str(tools / "node_modules/.bin/tsx")
                + '" --tsconfig "'
                + str(tools / "tsconfig.json")
                + '" "'
                + str(tools / "src/cli.ts")
                + '" "$@"\n'
            )
            wrapper.chmod(0o755)
            env = {
                key: value
                for key, value in os.environ.items()
                if not key.startswith("GIT_")
            }
            env.update(
                GIT_CONFIG_NOSYSTEM="1",
                GIT_CONFIG_GLOBAL=os.devnull,
                PATH=str(binary) + os.pathsep + env["PATH"],
                RUNNER_TEMP=str(runner),
            )

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
            git("config", "tag.gpgSign", "false")
            git("-c", "commit.gpgSign=false", "commit", "--allow-empty", "-m", "source")
            source = git("rev-parse", "HEAD")
            git("remote", "add", "origin", str(root / "missing-remote"))
            for sdk in ["node-sdk", "browser-sdk", "agent-sdk", "cli"]:
                with self.subTest(sdk=sdk):
                    subprocess.run(
                        ["bash", "-euc", script("Prepare release tag", npm)],
                        cwd=root,
                        env=dict(env, SDK=sdk, VERSION="1.2.3"),
                        check=True,
                        capture_output=True,
                    )
                    self.assertEqual(
                        git("bundle", "list-heads", str(runner / "release.bundle")),
                        f"{source} refs/tags/{sdk}-1.2.3",
                    )

    def test_npm_dry_run_and_publish_keep_tokens_isolated(self):
        npm = (ROOT / ".github/workflows/npm-publish.yml").read_text()
        preflight = (ROOT / ".github/workflows/check-release-push.yml").read_text()
        self.assertNotIn("GH_APP_PK", npm)
        self.assertNotIn("RELEASE_TOKEN", npm)
        self.assertIn("contents: read", npm)
        self.assertIn("ref: ${{ inputs.ref || github.ref }}", npm)
        self.assertIn("dry-run: ${{ inputs.dry-run }}", npm)
        self.assertIn(
            "source-sha: ${{ needs.check-push-permissions.outputs.source-sha }}", npm
        )
        verify = preflight.split("      - name: Verify release App permissions\n", 1)[1]
        self.assertIn("if: ${{ inputs.dry-run != true }}", verify)
        for name in ["Prepare release tag", "Store release tag"]:
            step = npm.split("      - name: " + name + "\n", 1)[1].split("      - ", 1)[
                0
            ]
            self.assertIn("if: ${{ inputs.dry-run != true }}", step)
            self.assertLess(
                npm.index("- name: " + name), npm.index("- name: Publish to NPM")
            )
        push = npm.split("  push-tag:\n", 1)[1]
        self.assertIn("if: ${{ inputs.dry-run != true }}", push)
        self.assertIn("needs: [publish, check-push-permissions]", push)
        self.assertIn("uses: ./.github/workflows/push-release-tag.yml", push)


if __name__ == "__main__":
    unittest.main()
