#!/usr/bin/env python3
"""Check that each worktree's stack runs that worktree's own backend build.

Every Nix build tags its image ghcr.io/xmtp/backend:self-hosted. If two
worktrees shared one image reference, Compose could recreate one worktree's
backend from the other worktree's build. These checks run dev/worktree-env in
real git worktrees, resolve each Compose file's backend image from the
generated .env, and check the loader that applies the tag and the recipes that
load and remove it. They do not use a Docker daemon.
"""

import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKTREE_ENV = ROOT / "dev/worktree-env"
COMPOSE_FILES = [ROOT / "dev/docker/compose.yml", ROOT / "dev/tls/compose.yml"]
NIX_TAG = "ghcr.io/xmtp/backend:self-hosted"
INTERPOLATION = re.compile(r"\$\{(\w+)(?:(:?-)([^}]*))?\}")


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


if __name__ == "__main__":
    unittest.main()
