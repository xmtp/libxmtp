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

import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/push-release-tag.yml"


def script(name, workflow=WORKFLOW):
    text = workflow.read_text().split("      - name: " + name + "\n", 1)[1]
    text = text.split("        run: |\n", 1)[1]
    lines = []
    for line in text.splitlines():
        if line.strip() and not line.startswith("          "):
            break
        lines.append(line)
    return dedent("\n".join(lines))


def release_tools(binary):
    """Write an xmtp-release command that runs the local CLI source."""
    tools = ROOT / "dev/release-tools"
    wrapper = binary / "xmtp-release"
    wrapper.write_text(
        f'#!/usr/bin/env bash\nexec "{tools}/node_modules/.bin/tsx" '
        f'--tsconfig "{tools}/tsconfig.json" "{tools}/src/cli.ts" "$@"\n'
    )
    wrapper.chmod(0o755)


def git_runner(env, cwd=None, strip=True):
    def git(*args, cwd=cwd):
        out = subprocess.run(
            ["git", *args], cwd=cwd, env=env, check=True, capture_output=True, text=True
        ).stdout
        return out.strip() if strip else out

    return git


class ReleasePushTest(unittest.TestCase):
    def test_release_source_stays_fixed_when_the_requested_branch_moves(self):
        def jobs(name):
            path = ROOT / f".github/workflows/{name}.yml"
            return yaml.safe_load(path.read_text())["jobs"]

        def expand(raw, values):
            keys = raw.removeprefix("${{ ").removesuffix(" }}").split(" || ")
            return next(filter(None, (values.get(key) for key in keys)), "")

        release, plan = jobs("release"), jobs("release-gate-plan")
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            env = os.environ | {
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_GLOBAL": os.devnull,
            }
            git = git_runner(env, directory)
            git("init", "--initial-branch=main")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.com")
            git("config", "commit.gpgSign", "false")
            git("commit", "--allow-empty", "-m", "original")
            original = git("rev-parse", "HEAD")
            output = directory / "output"

            def resolve(name, workflow):
                output.write_text("")
                source = ROOT / f".github/workflows/{workflow}.yml"
                subprocess.run(
                    ["bash", "-euc", script(name, source)],
                    cwd=directory,
                    env=env | {"GITHUB_OUTPUT": str(output)},
                    check=True,
                )
                return dict(
                    line.split("=", 1) for line in output.read_text().splitlines()
                )["sha"]

            def checkout(job, values):
                step = next(
                    s
                    for s in job["steps"]
                    if s.get("uses", "").startswith("actions/checkout@")
                )
                git("checkout", "--detach", expand(step["with"]["ref"], values))
                self.assertEqual(git("rev-parse", "HEAD"), original)

            frozen = resolve("Resolve release source", "release")
            self.assertEqual(frozen, original)
            git("commit", "--allow-empty", "-m", "branch advanced")
            self.assertNotEqual(git("rev-parse", "main"), original)
            values = {
                "needs.validate.outputs.source-sha": frozen,
                "needs.validate.outputs.ref": "main",
            }
            plan_ref = expand(release["plan"]["with"]["ref"], values)
            checkout(plan["gate"], {"inputs.ref": plan_ref})
            gate_sha = resolve("Resolve exact SHA", "release-gate-plan")
            checkout(
                plan["pending"],
                {"inputs.ref": plan_ref, "needs.gate.outputs.sha": gate_sha},
            )
            rn = {
                "inputs." + key: expand(raw, values)
                for key, raw in release["release-react-native-sdk"]["with"].items()
                if isinstance(raw, str)
            }
            source = jobs("release-react-native-sdk")["check-push-permissions"]["with"][
                "ref"
            ]
            git("checkout", "--detach", expand(source, rn))
            self.assertEqual(git("rev-parse", "HEAD"), original)
            checkout(release["hub"], values)
            # Every sibling receives the same snapshot while keeping its requested
            # ref. The existing CLI test below proves dev classification for each SDK.
            for name, job in release.items():
                if name.startswith("release-"):
                    with self.subTest(caller=name):
                        self.assertEqual(
                            expand(job["with"]["source-sha"], values), frozen
                        )
                        self.assertEqual(expand(job["with"]["ref"], values), "main")

    def test_production_version_scripts_keep_requested_ref_classification(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "bin"
            binary.mkdir()
            release_tools(binary)
            for path in [
                "sdks/node/package.json",
                "sdks/browser/package.json",
                "sdks/agent/package.json",
                "sdks/react-native/package.json",
                "apps/cli/package.json",
            ]:
                manifest = root / path
                manifest.parent.mkdir(parents=True, exist_ok=True)
                manifest.write_text('{"version":"8.0.0"}\n')
            (root / "sdks/android").mkdir(parents=True)
            (root / "sdks/android/gradle.properties").write_text("version=8.0.0\n")
            (root / "sdks/ios").mkdir(parents=True)
            (root / "sdks/ios/XMTP.podspec").write_text('spec.version = "8.0.0"\n')
            env = {
                key: value
                for key, value in os.environ.items()
                if not key.startswith("GIT_")
            }
            env.update(
                GIT_CONFIG_NOSYSTEM="1",
                GIT_CONFIG_GLOBAL=os.devnull,
                PATH=str(binary) + os.pathsep + env["PATH"],
            )

            git = git_runner(env, root)

            git("init", "--initial-branch=pinned")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.com")
            git(
                "-c",
                "commit.gpgSign=false",
                "commit",
                "--allow-empty",
                "-m",
                "pinned source",
            )
            source = git("rev-parse", "HEAD")
            short = git("rev-parse", "--short=7", "HEAD")
            timestamp = "20260102030405"
            for sdk in [
                "android",
                "ios",
                "node-sdk",
                "browser-sdk",
                "agent-sdk",
                "react-native-sdk",
                "cli",
            ]:
                workflow = ROOT / f".github/workflows/release-{sdk}.yml"
                version_step = (
                    workflow.read_text()
                    .split("      - name: Compute version\n", 1)[1]
                    .split("      - ", 1)[0]
                )
                ref_expression = re.search(r"(?m)^\s+REF: (.+)$", version_step).group(1)
                cases = [
                    ("main", "dev"),
                    ("refs/heads/main", "dev"),
                    ("release/8.0.0", "rc"),
                ]
                if sdk == "react-native-sdk":
                    cases.append(("main", "nightly"))
                for requested, release_type in cases:
                    with self.subTest(
                        sdk=sdk, requested=requested, release_type=release_type
                    ):
                        values = {
                            "inputs.ref": requested,
                            "inputs.ref || github.ref": requested,
                            "needs.check-push-permissions.outputs.source-sha": source,
                            "inputs.release-type": release_type,
                            "inputs.rc-number": "1",
                            "inputs.pending-version": "8.0.0",
                            "inputs.pending-kind": "patch",
                        }

                        if release_type == "nightly":
                            values["inputs.pending-version"] = "1.12.0"
                            values["inputs.pending-kind"] = "minor"

                        def expand(text):
                            return re.sub(
                                r"\$\{\{\s*(.*?)\s*\}\}",
                                lambda match: values[match.group(1)],
                                text,
                            )

                        output = root / "version-output"
                        output.write_text("")
                        subprocess.run(
                            [
                                "bash",
                                "-euc",
                                expand(script("Compute version", workflow)),
                            ],
                            cwd=root,
                            env=dict(
                                env,
                                REF=expand(ref_expression),
                                TIMESTAMP=timestamp,
                                RELEASE_TYPE=release_type,
                                RC_NUMBER="1",
                                PENDING_VERSION="8.0.0",
                                PENDING_KIND="patch",
                                GITHUB_OUTPUT=str(output),
                            ),
                            check=True,
                            capture_output=True,
                        )
                        actual = next(
                            line.removeprefix("version=")
                            for line in output.read_text().splitlines()
                            if line.startswith("version=")
                        )
                        expected = (
                            f"8.0.0-pre.{timestamp}.dev.{short}"
                            if release_type == "dev"
                            else f"8.1.0-pre.{timestamp}.nightly.{short}"
                            if release_type == "nightly"
                            else "8.0.0-rc1"
                        )
                        self.assertEqual(actual, expected)

    def test_release_source_is_the_checked_out_ref_not_the_caller(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env = {
                key: value
                for key, value in os.environ.items()
                if not key.startswith("GIT_")
            }
            env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
            git = git_runner(env, cwd=root)
            git("init", "--initial-branch=caller")
            for message in ("caller", "selected"):
                if message == "selected":
                    git("switch", "--quiet", "-c", "release")
                git(
                    "-c",
                    "user.name=T",
                    "-c",
                    "user.email=t@example.com",
                    "-c",
                    "commit.gpgSign=false",
                    "commit",
                    "--allow-empty",
                    "-m",
                    message,
                )
            caller_sha = git("rev-parse", "caller")
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
            # Every release caller builds, tags, and publishes this commit.
            self.assertEqual(output.read_text(), f"sha={git('rev-parse', 'release')}\n")

    def test_release_source_must_be_on_trunk_or_a_release_branch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env = {
                key: value
                for key, value in os.environ.items()
                if not key.startswith("GIT_")
            }
            env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)

            git = git_runner(env)

            work = root / "work"
            work.mkdir()
            git("init", "--initial-branch=main", cwd=work)
            commits = {}
            for branch in (
                "main",
                "self-hosted",
                "release/1.0",
                "feature",
                "release-notes",
            ):
                if branch != "main":
                    git("switch", "--quiet", "-C", branch, "main", cwd=work)
                git(
                    "-c",
                    "user.name=T",
                    "-c",
                    "user.email=t@e",
                    "-c",
                    "commit.gpgSign=false",
                    "commit",
                    "--allow-empty",
                    "-m",
                    branch,
                    cwd=work,
                )
                commits[branch] = git("rev-parse", "HEAD", cwd=work)
            origin = root / "origin.git"
            git("clone", "--quiet", "--bare", str(work), str(origin), cwd=root)
            git("config", "uploadpack.allowFilter", "true", cwd=origin)
            check = script(
                "Check that the source is on a release branch",
                ROOT / ".github/workflows/check-release-push.yml",
            )
            for branch, allowed in (
                ("main", True),
                ("self-hosted", False),
                ("release/1.0", True),
                ("feature", False),
                ("release-notes", False),
            ):
                with self.subTest(branch=branch):
                    # A shallow checkout of the source, as actions/checkout makes.
                    clone = root / f"clone-{branch.replace('/', '-')}"
                    git(
                        "clone",
                        "--quiet",
                        "--depth=1",
                        "--no-tags",
                        f"file://{origin}",
                        str(clone),
                        "--branch",
                        branch,
                        cwd=root,
                    )
                    result = subprocess.run(
                        ["bash", "-euc", check],
                        cwd=clone,
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(
                        result.returncode == 0, allowed, result.stdout + result.stderr
                    )
                    self.assertEqual(
                        git("rev-parse", "HEAD", cwd=clone), commits[branch]
                    )

    def test_bundle_push_retains_commit_without_persisting_token(self):
        # libxmtp is the nightly hub tag of release.yml.
        # One SDK of each release shape: SwiftPM tag, npm tag, and the hub tag.
        for sdk in ["ios", "node-sdk", "react-native-sdk", "libxmtp"]:
            with self.subTest(sdk=sdk):
                self.bundle_push(sdk)

    def bundle_push(self, sdk):
        requests = []
        tag = "v1.2.3" if sdk == "libxmtp" else f"{sdk}-1.2.3"
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

            git = git_runner(env, source, strip=False)

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
            git("-c", "tag.gpgSign=false", "tag", tag)
            hook = source / ".git/hooks/pre-push"
            hook.write_text("#!/bin/sh\nexit 99\n")
            hook.chmod(0o755)
            git(
                "bundle",
                "create",
                str(temporary / "release-tag/release.bundle"),
                f"refs/tags/{tag}",
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
                    git("--git-dir=" + str(remote), "rev-parse", tag).strip(),
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
                self.check_calling_run(git, root, remote, step_env)
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

    def check_calling_run(self, git, root, remote, step_env):
        """Only a protected trunk or release branch run may mint the token."""
        git("push", "--no-verify", str(remote), "main:main", "main:self-hosted")
        other = root / "fixture/other.git"
        work = root / "other-work"
        git("init", "--initial-branch=main", str(work))
        git(
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "--allow-empty",
            "-m",
            "unrelated",
            cwd=work,
        )
        git("init", "--bare", str(other))
        git(
            "push", "--no-verify", str(other), "main:main", "main:self-hosted", cwd=work
        )
        trunk = {"EVENT_NAME": "push", "CALLER_REF": "refs/heads/main"}
        for change, allowed in [
            ({}, True),
            ({"CALLER_REF": "refs/heads/self-hosted"}, False),
            ({"EVENT_NAME": "schedule"}, True),
            (
                {
                    "EVENT_NAME": "workflow_dispatch",
                    "CALLER_REF": "refs/heads/release/1.0",
                },
                True,
            ),
            ({"EVENT_NAME": "pull_request"}, False),
            ({"EVENT_NAME": "pull_request_target"}, False),
            ({"CALLER_REF": "refs/heads/feature"}, False),
            ({"CALLER_REF": "refs/tags/v1.2.3"}, False),
            ({"REF_PROTECTED": "false"}, False),
            # The source commit is on no release branch of the repository.
            ({"GITHUB_REPOSITORY": "fixture/other"}, False),
        ]:
            with self.subTest(change=change):
                result = subprocess.run(
                    ["bash", "-euc", script("Check the calling run")],
                    cwd=root,
                    env=step_env | {"REF_PROTECTED": "true"} | trunk | change,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(
                    result.returncode == 0, allowed, result.stdout + result.stderr
                )

    def test_rejects_a_release_commit_that_changes_workflows(self):
        for sdk in [
            "ios",
            "android",
            "node-sdk",
            "browser-sdk",
            "agent-sdk",
            "react-native-sdk",
            "cli",
        ]:
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

                git = git_runner(env, root)

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
        # The App secrets live in the `release` environment, which admits only
        # main and release/*, so a pull request run cannot mint
        # the token. Every job that reads them runs in that environment.
        readers = []
        for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
            jobs = path.read_text().split("\njobs:\n", 1)[-1]
            for body in re.split(r"(?m)^  [a-z][a-z0-9-]*:\n", jobs):
                if "GH_APP_ID" in body or "GH_APP_PK" in body:
                    readers.append(path.name)
                    with self.subTest(workflow=path.name):
                        # check-release-push.yml skips the token on a dry run.
                        self.assertRegex(
                            body,
                            r"(?m)^    environment: (release|\$\{\{ inputs\.dry-run "
                            r"!= true && 'release' \|\| '' \}\})$",
                        )
        self.assertEqual(
            sorted(readers), ["check-release-push.yml", "push-release-tag.yml"]
        )
        self.assertNotIn("checkout@", text)
        self.assertNotIn("xmtp-release", text)
        self.assertNotIn("uses: ./", text)
        self.assertLess(
            text.index("- name: Import release tag"),
            text.index("- name: Check the calling run"),
        )
        self.assertLess(
            text.index("- name: Check the calling run"),
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
                self.assertIn("REF: ${{ inputs.ref }}", mobile)
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
            release_tools(binary)
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

            git = git_runner(env, root)

            git("init", "--initial-branch=main")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.com")
            git("config", "tag.gpgSign", "false")
            git("-c", "commit.gpgSign=false", "commit", "--allow-empty", "-m", "source")
            previous_source = git("rev-parse", "HEAD")
            git(
                "-c",
                "commit.gpgSign=false",
                "commit",
                "--allow-empty",
                "-m",
                "selected",
            )
            source = git("rev-parse", "HEAD")
            git("remote", "add", "origin", str(root / "missing-remote"))
            # One SDK package and the CLI package share the job.
            for sdk in ["node-sdk", "cli"]:
                with self.subTest(sdk=sdk):
                    subprocess.run(
                        ["bash", "-euc", script("Prepare release tag", npm)],
                        cwd=root,
                        env=dict(env, SDK=sdk, VERSION="1.2.3", SOURCE_SHA=source),
                        check=True,
                        capture_output=True,
                    )
                    self.assertEqual(
                        git("bundle", "list-heads", str(runner / "release.bundle")),
                        f"{source} refs/tags/{sdk}-1.2.3",
                    )
                    tag = f"{sdk}-1.2.3"
                    # Retries may use either tag type at the selected source.
                    for annotated in [False, True]:
                        with self.subTest(annotated=annotated):
                            if annotated:
                                git("tag", "--delete", tag)
                                git("tag", "-a", tag, "-m", "release", source)
                            subprocess.run(
                                ["bash", "-euc", script("Prepare release tag", npm)],
                                cwd=root,
                                env=dict(
                                    env, SDK=sdk, VERSION="1.2.3", SOURCE_SHA=source
                                ),
                                check=True,
                                capture_output=True,
                            )
                            self.assertEqual(
                                git("rev-parse", f"{tag}^{{commit}}"), source
                            )
                            (runner / "release.bundle").unlink()
                            git("tag", "--delete", tag)
                            arguments = ["tag", tag, previous_source]
                            if annotated:
                                arguments = [
                                    "tag",
                                    "-a",
                                    tag,
                                    "-m",
                                    "old release",
                                    previous_source,
                                ]
                            git(*arguments)
                            result = subprocess.run(
                                ["bash", "-euc", script("Prepare release tag", npm)],
                                cwd=root,
                                env=dict(
                                    env, SDK=sdk, VERSION="1.2.3", SOURCE_SHA=source
                                ),
                                capture_output=True,
                                text=True,
                            )
                            self.assertNotEqual(result.returncode, 0)
                            self.assertIn("Release tag does not match", result.stdout)
                            self.assertFalse((runner / "release.bundle").exists())
                            git("tag", "--delete", tag)
                            git("tag", tag, source)

                    # Check moved HEAD with a fresh tag and a matching retry tag.
                    git(
                        "-c",
                        "commit.gpgSign=false",
                        "commit",
                        "--allow-empty",
                        "-m",
                        "moved",
                    )
                    for version in ["1.2.3", "1.2.4"]:
                        with self.subTest(moved_head_version=version):
                            (runner / "release.bundle").unlink(missing_ok=True)
                            result = subprocess.run(
                                ["bash", "-euc", script("Prepare release tag", npm)],
                                cwd=root,
                                env=dict(
                                    env, SDK=sdk, VERSION=version, SOURCE_SHA=source
                                ),
                                capture_output=True,
                                text=True,
                            )
                            self.assertNotEqual(result.returncode, 0)
                            self.assertIn("selected source commit", result.stdout)
                            self.assertFalse((runner / "release.bundle").exists())
                    git("reset", "--hard", source)

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
        prepare = npm.split("      - name: Prepare release tag\n", 1)[1].split(
            "      - ", 1
        )[0]
        self.assertIn(
            "SOURCE_SHA: ${{ needs.check-push-permissions.outputs.source-sha }}",
            prepare,
        )
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
        # The direct input also blocks Slack if validation exits without outputs.
        release = (ROOT / ".github/workflows/release.yml").read_text()
        notify = release.split("  notify:\n", 1)[1].split("    steps:\n", 1)[0]
        self.assertIn("inputs.dry-run != true", notify)
        self.assertIn("needs.validate.outputs.dry-run != 'true'", notify)

    def test_npm_callers_fix_source_before_setup_and_build(self):
        source = "${{ needs.check-push-permissions.outputs.source-sha }}"
        for name in ["node-sdk", "browser-sdk", "agent-sdk", "react-native-sdk", "cli"]:
            with self.subTest(caller=name):
                text = (ROOT / f".github/workflows/release-{name}.yml").read_text()
                jobs = re.split(
                    r"(?m)^  ([a-z][a-z0-9-]*):\n", text.split("jobs:\n", 1)[1]
                )
                bodies = dict(zip(jobs[1::2], jobs[2::2]))
                preflight = bodies["check-push-permissions"]
                self.assertIn(
                    "uses: ./.github/workflows/check-release-push.yml", preflight
                )
                self.assertIn("dry-run: ${{ inputs.dry-run }}", preflight)
                for job in ["setup", "build", "publish"]:
                    body = bodies[job]
                    self.assertRegex(
                        body, r"needs: \[[^\n]*check-push-permissions[^\n]*\]"
                    )
                    self.assertIn("ref: " + source, body)
                    self.assertNotIn("ref: ${{ inputs.ref", body)
                requested = (
                    "${{ inputs.ref || github.ref }}"
                    if name in ["agent-sdk", "cli"]
                    else "${{ inputs.ref }}"
                )
                self.assertIn("REF: " + requested, bodies["setup"])


if __name__ == "__main__":
    unittest.main()
