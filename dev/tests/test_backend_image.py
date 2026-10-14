#!/usr/bin/env python3
"""Check that each worktree's stack runs that worktree's own backend build.

Every Nix build tags its image ghcr.io/xmtp/backend:self-hosted. If two
worktrees shared one image reference, Compose could recreate one worktree's
backend from the other worktree's build. These checks run dev/worktree-env in
real git worktrees, resolve each Compose file's backend image from the
generated .env, and check the loader that applies the tag and the recipes that
load and remove it. They do not use a Docker daemon.
"""

import io
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKTREE_ENV = ROOT / "dev/worktree-env"
LOADER = ROOT / "dev/docker/load-backend-image"
COMPOSE_FILES = [ROOT / "dev/docker/compose.yml", ROOT / "dev/tls/compose.yml"]
NIX_TAG = "ghcr.io/xmtp/backend:self-hosted"
INTERPOLATION = re.compile(r"\$\{(\w+)(?:(:?-)([^}]*))?\}")
FAKE_DOCKER = r"""#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys

Path(os.environ["DOCKER_ARGS"]).write_text(json.dumps(sys.argv[1:]))
status = int(os.environ.get("DOCKER_STATUS", "0"))
if status:
    sys.exit(status)
Path(os.environ["DOCKER_INPUT"]).write_bytes(sys.stdin.buffer.read())
Path(os.environ["DOCKER_INPUT"] + ".complete").touch()
"""


def clean_environment(**overrides):
    """The caller's environment without stack settings or git redirection."""
    environment = {
        name: value
        for name, value in os.environ.items()
        if not name.startswith(("XMTP_", "GIT_"))
        and name not in ("DATABASE_URL", "ANVIL_URL")
    }
    environment.update(overrides)
    return environment


def git(*args):
    subprocess.run(
        [
            "git",
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.com",
            *args,
        ],
        check=True,
        capture_output=True,
        env=clean_environment(),
    )


def executable(path, body):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + body)
    path.chmod(0o755)


def read_env(checkout):
    """The values in a checkout's dev/docker/.env, one NAME='value' per line."""
    text = (checkout / "dev/docker/.env").read_text()
    return dict(token.split("=", 1) for token in shlex.split(text, comments=True))


def fixture_worktrees(test):
    """A main checkout and two worktrees under one parent, with worktree-env."""
    temporary = tempfile.TemporaryDirectory()
    test.addCleanup(temporary.cleanup)
    base = Path(temporary.name).resolve()
    main = base / "main"
    main.mkdir()
    git("-C", str(main), "init", "-q")
    git("-C", str(main), "commit", "-q", "--allow-empty", "-m", "fixture")
    # One parent for both worktrees, so both derive the same name and only the
    # slot can tell them apart.
    lanes = [base / "lanes/one", base / "lanes/two"]
    for lane in lanes:
        git("-C", str(main), "worktree", "add", "-q", "--detach", str(lane))
    for checkout in [main, *lanes]:
        (checkout / "dev/docker").mkdir(parents=True)
        shutil.copy2(WORKTREE_ENV, checkout / "dev/worktree-env")
        shutil.copy2(ROOT / "dev/docker/load-env", checkout / "dev/docker/load-env")
    return base, main, lanes


def compose_backend_image(compose_file, environment):
    """Resolve the backend image the way Compose interpolates it."""
    image = yaml.safe_load(compose_file.read_text())["services"]["backend"]["image"]

    def substitute(match):
        name, operator, default = match.groups()
        value = environment.get(name)
        if operator == ":-":
            return value or default
        if operator == "-":
            return default if value is None else value
        return value or ""

    return INTERPOLATION.sub(substitute, image)


class WorktreeImageTest(unittest.TestCase):
    def setUp(self):
        self.base, self.main, self.lanes = fixture_worktrees(self)

    def generate(self, checkout, **overrides):
        """Run dev/worktree-env in a checkout and return the generated values."""
        subprocess.run(
            [str(checkout / "dev/worktree-env")],
            check=True,
            capture_output=True,
            text=True,
            env=clean_environment(**overrides),
            timeout=60,
        )
        return read_env(checkout)

    def test_two_worktrees_run_distinct_backend_images(self):
        first, second = (self.generate(lane) for lane in self.lanes)
        self.assertNotEqual(first["XMTP_WORKTREE_SLOT"], second["XMTP_WORKTREE_SLOT"])
        for compose_file in COMPOSE_FILES:
            with self.subTest(compose_file=compose_file.relative_to(ROOT)):
                images = [
                    compose_backend_image(compose_file, env) for env in (first, second)
                ]
                self.assertNotEqual(images[0], images[1])
                self.assertNotIn(NIX_TAG, images)
                # The loader tags the build with XMTP_BACKEND_IMAGE, so Compose
                # must resolve exactly that reference.
                self.assertEqual(
                    images, [first["XMTP_BACKEND_IMAGE"], second["XMTP_BACKEND_IMAGE"]]
                )

    def test_main_checkout_does_not_use_the_nix_tag(self):
        values = self.generate(self.main)
        self.assertEqual(values["XMTP_WORKTREE_SLOT"], "0")
        self.assertEqual(values["XMTP_BACKEND_IMAGE"], "xmtp-backend:libxmtp")
        for compose_file in COMPOSE_FILES:
            self.assertEqual(
                compose_backend_image(compose_file, values), "xmtp-backend:libxmtp"
            )

    def test_an_explicit_image_wins(self):
        published = "ghcr.io/xmtp/backend:8.0.0-rc1"
        values = self.generate(self.lanes[0], XMTP_BACKEND_IMAGE=published)
        for compose_file in COMPOSE_FILES:
            self.assertEqual(compose_backend_image(compose_file, values), published)

    def test_tls_stack_gets_this_worktree_image(self):
        # dev/tls/compose.yml has no .env beside it, so only the wrapper's
        # exports reach it. A direct call would resolve the Nix tag instead.
        for script in ("up", "check.sh"):
            text = (ROOT / "dev/tls" / script).read_text()
            self.assertIn('"$script_dir/compose" up', text)
            self.assertNotIn("docker compose", text)
        lane = self.lanes[0]
        (lane / "dev/tls").mkdir()
        shutil.copy2(ROOT / "dev/tls/compose", lane / "dev/tls/compose")
        record = self.base / "docker-call"
        executable(
            self.base / "bin/docker",
            'printf "%s\\n" "$XMTP_BACKEND_IMAGE" "$@" > "$RECORD"\n',
        )
        subprocess.run(
            [str(lane / "dev/tls/compose"), "config"],
            check=True,
            capture_output=True,
            env=clean_environment(
                PATH=str(self.base / "bin") + os.pathsep + os.environ["PATH"],
                RECORD=str(record),
            ),
            timeout=60,
        )
        image, *args = record.read_text().splitlines()
        self.assertEqual(image, read_env(lane)["XMTP_BACKEND_IMAGE"])
        self.assertNotEqual(image, NIX_TAG)
        self.assertEqual(
            args,
            [
                "compose",
                "-f",
                str(lane / "dev/tls/compose.yml"),
                "-p",
                "libxmtp-tls",
                "config",
            ],
        )


class RecipeTest(unittest.TestCase):
    """`just backend` loads and removes only the tag that worktree-env derives."""

    DERIVED = "xmtp-backend:libxmtp-lanes-7"
    PUBLISHED = "ghcr.io/xmtp/backend:8.0.0-rc1"

    def setUp(self):
        base, _, lanes = fixture_worktrees(self)
        self.root = lanes[0]
        names = subprocess.check_output(
            ["git", "ls-files", "*.just", "justfile"], cwd=ROOT, text=True
        ).splitlines()
        for name in names:
            (self.root / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, self.root / name)
        self.log = base / "calls.log"
        executable(self.root / "dev/nix-shell", 'exec bash -euc "$1"\n')
        executable(
            self.root / "dev/docker/load-backend-image", 'echo "load $*" >> "$LOG"\n'
        )
        executable(self.root / "dev/docker/compose", 'echo "$*" >> "$LOG"\n')
        executable(
            base / "bin/docker",
            'case "$1 $2" in\n'
            '  "image inspect") exit 0 ;;\n'
            '  "image rm") echo "rm $3" >> "$LOG"; exit "${RM_STATUS:-0}" ;;\n'
            "esac\n"
            'echo "unexpected docker $*" >&2\n'
            "exit 1\n",
        )
        self.environment = clean_environment(
            PATH=str(base / "bin") + os.pathsep + os.environ["PATH"],
            LOG=str(self.log),
            XMTP_WORKTREE_SLOT="7",
        )

    def just(self, *args, **overrides):
        result = subprocess.run(
            ["just", "--justfile", str(self.root / "justfile"), *args],
            cwd=self.root,
            env=dict(self.environment, **overrides),
            capture_output=True,
            text=True,
            timeout=120,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_up_loads_the_build_under_the_derived_tag(self):
        self.just("--no-deps", "backend", "_load-image")
        self.assertEqual(
            self.calls(), [f"load {self.root}/result-backend-image {self.DERIVED}"]
        )

    def test_an_overridden_image_never_receives_a_local_build(self):
        result = self.just(
            "--no-deps", "backend", "_load-image", XMTP_BACKEND_IMAGE=self.PUBLISHED
        )
        self.assertEqual(self.calls(), [])
        self.assertIn(self.PUBLISHED, result.stderr)

    def test_release_removes_the_derived_tag_and_frees_the_slot(self):
        self.just("backend", "release")
        self.assertEqual(self.calls(), ["down --volumes", f"rm {self.DERIVED}"])
        # `worktree-env --release` runs last and deletes the file.
        self.assertFalse((self.root / "dev/docker/.env").exists())

    def test_release_keeps_an_overridden_image(self):
        self.just("backend", "release", XMTP_BACKEND_IMAGE=self.PUBLISHED)
        self.assertEqual(self.calls(), ["down --volumes"])
        self.assertFalse((self.root / "dev/docker/.env").exists())

    def test_release_frees_the_slot_when_the_tag_is_in_use(self):
        result = self.just("backend", "release", RM_STATUS="1")
        self.assertIn(f"kept {self.DERIVED}", result.stderr)
        self.assertFalse((self.root / "dev/docker/.env").exists())


def archive_members(manifest=True):
    """A small Nix-style image archive: a layer, its config, and the manifest."""
    members = [
        ("0f1e/", None),
        ("0f1e/layer.tar", b"layer bytes" * 4096),
        ("c0nf.json", b'{"architecture": "arm64"}'),
    ]
    if manifest:
        entry = {
            "Config": "c0nf.json",
            "RepoTags": [NIX_TAG],
            "Layers": ["0f1e/layer.tar"],
        }
        members.append(("manifest.json", json.dumps([entry]).encode()))
    return members


def write_archive(path, members):
    with tarfile.open(path, "w:gz") as archive:
        for name, data in members:
            member = tarfile.TarInfo(name)
            if data is None:
                member.type = tarfile.DIRTYPE
                archive.addfile(member)
            else:
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))


class LoaderTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        bin_directory = self.directory / "bin"
        bin_directory.mkdir()
        docker = bin_directory / "docker"
        docker.write_text(FAKE_DOCKER)
        docker.chmod(0o755)
        self.archive = self.directory / "backend.tar.gz"
        self.input = self.directory / "loaded.tar"
        self.args = self.directory / "args.json"
        self.environment = dict(
            os.environ,
            PATH=str(bin_directory) + os.pathsep + os.environ["PATH"],
            DOCKER_ARGS=str(self.args),
            DOCKER_INPUT=str(self.input),
        )

    def load(self, image="xmtp-backend:libxmtp-lanes-1", **overrides):
        return subprocess.run(
            [str(LOADER), str(self.archive), image],
            env=dict(self.environment, **overrides),
            capture_output=True,
            text=True,
            timeout=30,
        )

    def test_loads_the_archive_under_only_the_requested_tag(self):
        members = archive_members()
        write_archive(self.archive, members)
        result = self.load()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(self.args.read_text()), ["load"])
        with tarfile.open(self.input) as loaded:
            names = loaded.getnames()
            contents = {
                member.name: loaded.extractfile(member).read()
                for member in loaded
                if member.isfile()
            }
        self.assertEqual(names, [name.rstrip("/") for name, _ in members])
        manifest = json.loads(contents.pop("manifest.json"))
        self.assertEqual(manifest[0]["RepoTags"], ["xmtp-backend:libxmtp-lanes-1"])
        self.assertEqual(manifest[0]["Layers"], ["0f1e/layer.tar"])
        self.assertEqual(
            contents, {name: data for name, data in members[:-1] if data is not None}
        )

    def test_an_archive_without_a_manifest_is_never_loaded(self):
        write_archive(self.archive, archive_members(manifest=False))
        result = self.load()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no manifest.json", result.stderr)
        self.assertFalse(Path(str(self.input) + ".complete").exists())

    def test_a_docker_failure_is_the_loader_failure(self):
        write_archive(self.archive, archive_members())
        result = self.load(DOCKER_STATUS="3")
        self.assertEqual(result.returncode, 3, result.stderr)

    def test_a_missing_archive_fails_without_a_traceback(self):
        result = self.load()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("load-backend-image:", result.stderr)
        self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
