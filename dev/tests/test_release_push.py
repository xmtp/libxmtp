#!/usr/bin/env python3
"""Check isolated release tag transfer with a local Git HTTP remote."""

import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.machinery
import importlib.util
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
        f'#!/usr/bin/env bash\nexport TSX_TSCONFIG_PATH="{tools}/tsconfig.json"\n'
        f'exec node --import "{tools}/node_modules/tsx/dist/loader.mjs" "{tools}/src/cli.ts" "$@"\n'
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

        release = jobs("release")
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
            checkout(release["release-xdbg"], values)
            # Every sibling receives the same snapshot. Only the backend keeps
            # the requested ref; the SDK versions were classified in validate,
            # which the CLI test below runs.
            for name, job in release.items():
                if name.startswith("release-") and "uses" in job:
                    with self.subTest(caller=name):
                        self.assertEqual(
                            expand(job["with"]["source-sha"], values), frozen
                        )
                        if name == "release-backend":
                            self.assertEqual(expand(job["with"]["ref"], values), "main")
                        else:
                            self.assertNotIn("ref", job["with"])

    def test_production_script_freezes_one_version_for_all_sdk_callers(self):
        import shutil

        workflow = ROOT / ".github/workflows/release.yml"
        jobs = yaml.safe_load(workflow.read_text())["jobs"]
        sdks = [
            "android",
            "ios",
            "node-sdk",
            "browser-sdk",
            "agent-sdk",
            "react-native-sdk",
            "cli",
        ]
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name in [
                "Cargo.toml",
                "Cargo.lock",
                "crates/xmtp_sdk/Cargo.toml",
                "sdks/android/gradle.properties",
                "apps/cli/package.json",
                *[
                    f"sdks/{sdk}/package.json"
                    for sdk in ("node", "browser", "agent", "react-native")
                ],
            ]:
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / name, destination)
            (root / "dev").mkdir()
            shutil.copyfile(ROOT / "dev/sdk_version.py", root / "dev/sdk_version.py")
            subprocess.run(
                ["python3.11", "dev/sdk_version.py", "set", "8.0.0"],
                cwd=root,
                check=True,
            )
            shell = root / "dev/nix-shell"
            shell.write_text(
                '#!/usr/bin/env python3\nimport os,sys\ni=sys.argv.index("--command")+1\nos.execvp(sys.argv[i],sys.argv[i:])\n'
            )
            shell.chmod(0o755)
            env = os.environ | {
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_CONFIG_GLOBAL": os.devnull,
            }
            git = git_runner(env, root)
            git("init", "--initial-branch=main")
            git("config", "user.name", "Test")
            git("config", "user.email", "test@example.com")
            git("add", ".")
            git("-c", "commit.gpgSign=false", "commit", "-m", "source")
            sha = git("rev-parse", "HEAD")
            output = Path(folder).parent / (root.name + "-output")
            self.addCleanup(lambda: output.unlink(missing_ok=True))
            timestamp = "20260102030405"
            for requested, kind, expected in [
                ("main", "dev", f"8.0.0-pre.{timestamp}.dev.g{sha[:7]}"),
                ("feature", "dev", f"8.0.0-pre.{timestamp}.dev.g{sha[:7]}"),
                ("release/8.0.0", "rc", "8.0.0-rc5"),
                ("release/8.0.0", "final", "8.0.0"),
            ]:
                with self.subTest(ref=requested, kind=kind):
                    output.write_text("")
                    subprocess.run(
                        ["bash", "-euc", script("Compute SDK version once", workflow)],
                        cwd=root,
                        env=env
                        | {
                            "GITHUB_OUTPUT": str(output),
                            "RUNNER_TEMP": folder,
                            "REF": requested,
                            "TIMESTAMP": timestamp,
                            "RELEASE_TYPE": kind,
                            "RC_NUMBER": "5",
                            "CLI": "false",
                        },
                        check=True,
                        capture_output=True,
                    )
                    values = dict(
                        line.split("=", 1) for line in output.read_text().splitlines()
                    )
                    self.assertEqual(
                        values, {f"version-{sdk}": expected for sdk in sdks}
                    )
                    for sdk in sdks:
                        self.assertEqual(
                            jobs[f"release-{sdk}"]["with"]["version"],
                            f"${{{{ needs.validate.outputs.version-{sdk} }}}}",
                        )
                        self.assertEqual(
                            jobs["validate"]["outputs"][f"version-{sdk}"],
                            f"${{{{ steps.versions.outputs.version-{sdk} }}}}",
                        )
                    import json

                    record = json.loads((root / "release-plan.json").read_text())
                    self.assertEqual(record["sourceSha"], sha)
                    self.assertEqual(record["version"], expected)
            agent = yaml.safe_load(
                (workflow.parent / "release-agent-sdk.yml").read_text()
            )
            self.assertNotIn("workflow_dispatch", agent[True])

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
            # The npm publish and xdbg image jobs run the same check.
            for publisher in ("npm-publish.yml", "release.yml"):
                with self.subTest(publisher=publisher):
                    self.assertEqual(
                        script(
                            "Check that the source is on a release branch",
                            ROOT / ".github/workflows" / publisher,
                        ).strip(),
                        check.strip(),
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
        # One SDK of each release shape: SwiftPM tag and npm tags.
        for sdk in ["ios", "node-sdk", "react-native-sdk"]:
            with self.subTest(sdk=sdk):
                self.bundle_push(sdk)

    def bundle_push(self, sdk):
        requests = []
        tag = f"{sdk}-1.2.3"
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
            # No scheduled release remains, so a schedule run cannot push.
            ({"EVENT_NAME": "schedule"}, False),
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
                self.assertIn(f"dev/prepare_native_release.py {sdk}", mobile)
                self.assertIn("fetch-depth: 0", mobile)
                self.assertIn(f"name: release-tag-{sdk}", mobile)
                self.assertIn("uses: ./.github/workflows/push-release-tag.yml", mobile)
                source = "${{ needs.check-push-permissions.outputs.source-sha }}"
                self.assertIn("source-sha: " + source, mobile)
                self.assertNotIn("source-sha: ${{ github.sha }}", mobile)
                self.assertNotIn("inputs.ref", mobile)
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
        jobs = yaml.safe_load(ios)["jobs"]
        self.assertEqual(
            set(jobs["publish"]["needs"]), {"prepare-release", "check-push-permissions"}
        )
        self.assertIn("publish", jobs["push-tag"]["needs"])
        self.assertIn("push-tag", jobs["publish-release"]["needs"])
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
        # The caller's preflight checked the source; npm-publish takes it as a
        # required input and does not check it again.
        self.assertNotIn("uses: ./.github/workflows/check-release-push.yml", npm)
        inputs = yaml.safe_load(npm)[True]["workflow_call"]["inputs"]
        self.assertTrue(inputs["source-sha"]["required"])
        self.assertNotIn("ref", inputs)
        self.assertIn("source-sha: ${{ inputs.source-sha }}", npm)
        verify = preflight.split("      - name: Verify release App permissions\n", 1)[1]
        self.assertIn("if: ${{ inputs.dry-run != true }}", verify)
        prepare = npm.split("      - name: Prepare release tag\n", 1)[1].split(
            "      - ", 1
        )[0]
        self.assertIn("SOURCE_SHA: ${{ inputs.source-sha }}", prepare)
        for name in ["Prepare release tag", "Store release tag"]:
            step = npm.split("      - name: " + name + "\n", 1)[1].split("      - ", 1)[
                0
            ]
            self.assertIn("inputs.dry-run != true", step)
            self.assertIn("inputs.prepare-only != true", step)
            self.assertLess(
                npm.index("- name: " + name), npm.index("- name: Publish to NPM")
            )
        push = npm.split("  push-tag:\n", 1)[1]
        self.assertIn("inputs.dry-run != true", push)
        self.assertIn("inputs.prepare-only != true", push)
        self.assertIn("needs: [publish]", push)
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
                self.assertIn(
                    "ref: ${{ inputs.source-sha }}",
                    preflight,
                )
                # Every job after the preflight builds and tags its pinned
                # source, never the requested ref.
                for job, body in bodies.items():
                    if job == "check-push-permissions":
                        continue
                    self.assertRegex(
                        body, r"needs: \[[^\n]*check-push-permissions[^\n]*\]"
                    )
                    self.assertIn(source, body)
                    self.assertNotIn("inputs.ref", body)
                self.assertIn("source-sha: " + source, bodies["publish"])
                self.assertNotIn("setup", bodies)


class XdbgImageTest(unittest.TestCase):
    def test_only_a_checked_release_source_pushes_the_image(self):
        release = ROOT / ".github/workflows/release.yml"
        job = yaml.safe_load(release.read_text())["jobs"]["release-xdbg"]
        steps = {step["name"]: step for step in job["steps"]}
        # A dev release may come from any branch: the image is pushed only
        # when the source commit is on main or a release/** branch, and never
        # on a dry run. The check itself is tested with check-release-push.yml.
        check = steps["Check that the source is on a release branch"]
        self.assertEqual(check["if"], "needs.validate.outputs.dry-run != 'true'")
        self.assertIs(check["continue-on-error"], True)
        checked = "steps.release-source.outcome == 'success'"
        self.assertEqual(check["id"], "release-source")
        self.assertEqual(steps["Log in to GHCR"]["if"], checked)
        build = steps["Build and push image"]["with"]
        self.assertEqual(build["push"], "${{ " + checked + " }}")
        names = [step["name"] for step in job["steps"]]
        self.assertLess(
            names.index("Check that the source is on a release branch"),
            names.index("Log in to GHCR"),
        )
        # The tag is the backend release version.
        self.assertEqual(
            steps["Extract image labels"]["with"]["tags"],
            "type=raw,value=${{ needs.release-backend.outputs.version }}",
        )
        # Nothing deploys the image.
        self.assertEqual(
            [s for s in job["steps"] if s.get("name", "").startswith("Deploy")], []
        )
        self.assertNotIn("TERRAFORM_TOKEN", release.read_text())


class KeepaliveProbeImageTest(unittest.TestCase):
    def test_push_paths_cover_every_input_of_the_image_build(self):
        # A change to any input of the image must republish it. The package
        # comes from the Dockerfile's `cargo build`; its crates are the path
        # dependencies that Cargo builds for it (no dev-dependencies).
        dockerfile = (ROOT / "apps/keepalive-probe/docker/Dockerfile").read_text()
        (package,) = re.findall(r"cargo build --release --package (\S+)", dockerfile)
        loader = importlib.machinery.SourceFileLoader(
            "ci_suites", str(ROOT / "dev/ci-suites")
        )
        suites = importlib.util.module_from_spec(
            importlib.util.spec_from_loader(loader.name, loader)
        )
        loader.exec_module(suites)
        graph = suites.CargoGraph(ROOT)
        crates, queue = set(), [graph.by_name[package]]
        while queue:
            crate = queue.pop()
            if crate not in crates:
                crates.add(crate)
                queue.extend(graph.dependencies(crate))
        workflow = ".github/workflows/push-keepalive-probe.yml"
        expected = {f"{crate}/**" for crate in crates} | {
            workflow,
            # xmtp_proto's build script compiles the workspace protos.
            "proto/**",
            # Workspace build settings: rustflags, the pinned toolchain that
            # rustup installs in the builder, and the lock file.
            ".cargo/config.toml",
            "rust-toolchain.toml",
            "Cargo.toml",
            "Cargo.lock",
            # `COPY . .` copies the context that it filters.
            ".dockerignore",
        }
        # PyYAML reads the `on` key as True.
        push = yaml.safe_load((ROOT / workflow).read_text())[True]["push"]
        self.assertEqual(sorted(push["paths"]), sorted(expected))

    def test_runs_of_one_ref_never_overlap(self):
        # Two overlapping main runs could finish out of order. One run per ref
        # runs at a time, and it is never cancelled. The default queue keeps
        # only the newest pending run; `queue: max` would keep more.
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/push-keepalive-probe.yml").read_text()
        )
        self.assertEqual(
            workflow.get("concurrency"),
            {
                "group": "${{ github.workflow }}-${{ github.ref }}",
                "cancel-in-progress": False,
            },
        )

    CHECK = "Check that no newer commit on main changes the image"

    def test_only_the_newest_image_commit_moves_latest(self):
        # GitHub does not guarantee that waiting runs start in push order, so
        # an older commit's run can run last. Every run pushes its sha tag;
        # `latest` moves only when no newer commit on main changes a trigger
        # path, because that commit's own run moves it.
        path = ROOT / ".github/workflows/push-keepalive-probe.yml"
        job = yaml.safe_load(path.read_text())["jobs"]["build"]
        steps = {step["name"]: step for step in job["steps"]}
        names = list(steps)
        meta = "Extract metadata (tags, labels)"
        self.assertLess(names.index(self.CHECK), names.index(meta))
        self.assertLess(names.index(meta), names.index("Build and push image"))
        self.assertEqual(steps[self.CHECK]["if"], "github.ref == 'refs/heads/main'")
        self.assertEqual(steps[self.CHECK]["shell"], "bash")
        self.assertEqual(
            steps[meta]["with"]["tags"].splitlines(),
            [
                "type=raw,value=${{ steps.vars.outputs.short_sha }}",
                "type=raw,value=latest,enable=${{ steps.latest.outputs.latest == 'true' }}",
            ],
        )
        run = script(self.CHECK, path)
        newer = ["docs/a.md", "nix/lib/shell-common.nix"]
        cases = {
            # (gh output lines, gh exit status): latest
            "identical": ((["identical", "0"], 0), "true"),
            "unrelated newer commits": ((["ahead", "2", *newer], 0), "true"),
            "newer image commit": (
                (["ahead", "3", *newer, "crates/xmtp_logging/src/lib.rs"], 0),
                "false",
            ),
            "renamed out of a trigger path": (
                (["ahead", "1", "docs/old.proto", "proto/old.proto"], 0),
                "false",
            ),
            "newer workflow edit": (
                (["ahead", "1", ".github/workflows/push-keepalive-probe.yml"], 0),
                "false",
            ),
            "file list may be cut off": ((["ahead", "300", *newer], 0), "false"),
            "main went backwards": ((["behind", "0"], 0), "false"),
            "diverged": ((["diverged", "4", *newer], 0), "false"),
            "api error": (([], 1), "false"),
        }
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            (temp / "bin").mkdir()
            gh = temp / "bin/gh"
            gh.write_text(
                '#!/bin/sh\necho "$@" > "$GH_ARGS"\ncat "$GH_REPLY"\nexit "$GH_EXIT"\n'
            )
            gh.chmod(0o755)
            for case, ((reply, status), latest) in cases.items():
                with self.subTest(case=case):
                    (temp / "reply").write_text("".join(f"{x}\n" for x in reply))
                    (temp / "output").write_text("")
                    done = subprocess.run(
                        ["bash", "-eo", "pipefail", "-c", run],
                        env=os.environ
                        | {
                            "PATH": f"{temp / 'bin'}:{os.environ['PATH']}",
                            "GH_ARGS": str(temp / "args"),
                            "GH_REPLY": str(temp / "reply"),
                            "GH_EXIT": str(status),
                            "GITHUB_OUTPUT": str(temp / "output"),
                            "GITHUB_REPOSITORY": "xmtp/libxmtp",
                            "SHA": "abc123",
                        },
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(done.returncode, 0, done.stderr)
                    self.assertEqual(
                        (temp / "output").read_text(), f"latest={latest}\n"
                    )
                    self.assertEqual(
                        "::notice title=latest not moved::" in done.stdout,
                        latest == "false",
                    )
                    self.assertTrue(
                        (temp / "args")
                        .read_text()
                        .startswith("api repos/xmtp/libxmtp/compare/abc123...main ")
                    )

    def test_latest_check_lists_the_trigger_paths(self):
        path = ROOT / ".github/workflows/push-keepalive-probe.yml"
        run = script(self.CHECK, path)
        patterns = run.split('case "$1" in', 1)[1].split(") return 0 ;;", 1)[0]
        listed = [
            pattern.strip().removesuffix("/*") + ("/**" if "/*" in pattern else "")
            for pattern in patterns.replace("\\", "").split("|")
        ]
        push = yaml.safe_load(path.read_text())[True]["push"]
        self.assertEqual(sorted(listed), sorted(push["paths"]))


class NpmPublishAuthorizationTest(unittest.TestCase):
    def test_a_real_publish_authorizes_its_own_run(self):
        # A workflow on another branch can call npm-publish.yml directly. A
        # real publish must still run in the protected `release` environment
        # and check its source before `npm publish`; a dry run needs neither.
        job = yaml.safe_load((ROOT / ".github/workflows/npm-publish.yml").read_text())[
            "jobs"
        ]["publish"]
        self.assertEqual(
            job["environment"], "${{ inputs.dry-run != true && 'release' || '' }}"
        )
        self.assertEqual(job["permissions"]["id-token"], "write")
        names = [step.get("name") for step in job["steps"]]
        check = names.index("Check that the source is on a release branch")
        self.assertLess(check, names.index("Publish to NPM"))
        publish_if = "${{ inputs.dry-run != true && inputs.prepare-only != true }}"
        self.assertEqual(job["steps"][check]["if"], publish_if)
        self.assertEqual(job["steps"][names.index("Publish to NPM")]["if"], publish_if)
        # Every other job that gets an OIDC token publishes nothing.
        for path in sorted((ROOT / ".github/workflows").glob("*.yml")):
            text = path.read_text()
            if "npm publish" in text and path.name != "npm-publish.yml":
                with self.subTest(workflow=path.name):
                    self.assertNotRegex(
                        text, r"(?m)^\s+run: npm publish(?!.*--dry-run)"
                    )


class NodeInstallSmokeTest(unittest.TestCase):
    def test_linux_smoke_runs_each_pair_and_fails_without_pairs(self):
        workflow = ROOT / ".github/workflows/build-sdk-node-platforms.yml"
        run = script(
            "Installed package on Linux without build-host libraries", workflow
        )
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            (temp / "bin").mkdir()
            docker = temp / "bin/docker"
            docker.write_text('#!/bin/sh\necho "$7" >> "$DOCKER_LOG"\n')
            docker.chmod(0o755)
            env = os.environ | {
                "PATH": f"{temp / 'bin'}:{os.environ['PATH']}",
                "RUNNER_TEMP": directory,
                "DOCKER_LOG": str(temp / "docker.log"),
            }
            pairs = (
                '[{"target":"linux-x64-gnu","image":"node:22-bookworm"},'
                '{"target":"linux-x64-musl","image":"node:22-alpine"}]'
            )
            done = subprocess.run(
                ["bash", "-c", run], env=env | {"PAIRS": pairs}, capture_output=True
            )
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertEqual(
                (temp / "docker.log").read_text().split(),
                ["node:22-bookworm", "node:22-alpine"],
            )
            for broken in ("null", "[]", "{"):
                with self.subTest(pairs=broken):
                    done = subprocess.run(
                        ["bash", "-c", run],
                        env=env | {"PAIRS": broken},
                        capture_output=True,
                    )
                    self.assertNotEqual(done.returncode, 0)


if __name__ == "__main__":
    unittest.main()
