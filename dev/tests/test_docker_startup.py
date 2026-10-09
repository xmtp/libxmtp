#!/usr/bin/env python3
"""Exercise Compose startup diagnostics without contacting a Docker daemon."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = Path(os.environ.get("DOCKER_STARTUP_HELPER", ROOT / "dev/docker/up"))
PROJECT = "libxmtp-startup-fixture-38"
DOCKER = r"""#!/usr/bin/env python3.11
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
with Path(os.environ["DOCKER_CALLS"]).open("a") as calls:
    calls.write(json.dumps(args) + "\n")
mode = os.environ.get("DIAGNOSTIC_FAILURE", "")
if args[:1] == ["ps"]:
    # The legacy stack warning is a read-only global check before startup.
    sys.exit(0)
if args[:1] == ["compose"]:
    project = args[args.index("-p") + 1]
    command = args[5:]
    if command[:1] == ["up"]:
        print("fixture startup", file=sys.stderr)
        sys.exit(int(os.environ["STARTUP_STATUS"]))
    if command[:1] == ["logs"]:
        if mode == "logs":
            print("fixture log failure", file=sys.stderr)
            sys.exit(12)
        print('backend | Error: relation "allocation_boundary" does not exist')
    elif command[:1] == ["ps"]:
        if mode == "ps":
            print("fixture ps failure", file=sys.stderr)
            sys.exit(13)
        if "--quiet" in command:
            if project == os.environ["EXPECTED_PROJECT"]:
                print("owned-backend\nowned-db")
            else:
                print("unrelated-backend")
        else:
            print("owned-backend Exited (37)\nowned-db healthy")
    else:
        raise SystemExit("Unexpected Compose command: " + repr(command))
elif args[:1] == ["inspect"]:
    container = args[-1]
    if mode == "inspect" and container == "owned-backend":
        print("fixture inspect failure", file=sys.stderr)
        sys.exit(14)
    if args[1:2] == ["--format"]:
        print(json.dumps({"name": container, "state": {"ExitCode": 37}}))
    else:
        print(json.dumps({"Config": {"Env": ["SECRET=fixture-private-value"]}}))
else:
    raise SystemExit("Unexpected Docker command: " + repr(args))
"""


class DockerStartupTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.repo = self.directory / "repo"
        self.docker_dir = self.repo / "dev/docker"
        self.docker_dir.mkdir(parents=True)
        shutil.copyfile(HELPER, self.docker_dir / "up")
        (self.docker_dir / "compose.yml").touch()
        (self.docker_dir / ".env").write_text(
            f"XMTP_COMPOSE_PROJECT='{PROJECT}'\n"
            "XMTP_WORKTREE_SLOT='38'\n"
            "XMTP_BACKEND_DB_PORT='56230'\n"
            "XMTP_BACKEND_URL='http://127.0.0.1:5810'\n"
        )
        worktree = self.repo / "dev/worktree-env"
        worktree.write_text('#!/usr/bin/env bash\nexit "${WORKTREE_STATUS:-0}"\n')
        worktree.chmod(0o755)
        self.bin = self.directory / "bin"
        self.bin.mkdir()
        docker = self.bin / "docker"
        docker.write_text(DOCKER)
        docker.chmod(0o755)
        self.runner = self.directory / "runner"
        self.runner.mkdir()
        self.calls = self.directory / "calls.jsonl"

    def run_up(self, status=37, failure="", runner=True, **overrides):
        env = dict(
            os.environ,
            PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
            STARTUP_STATUS=str(status),
            DIAGNOSTIC_FAILURE=failure,
            DOCKER_CALLS=str(self.calls),
            EXPECTED_PROJECT=PROJECT,
            TMPDIR=str(self.runner),
        )
        env.pop("RUNNER_TEMP", None)
        if runner:
            env["RUNNER_TEMP"] = str(self.runner)
        env.update(overrides)
        return subprocess.run(
            ["bash", str(self.docker_dir / "up"), "backend", "replica"],
            env=env,
            text=True,
            capture_output=True,
            timeout=15,
        )

    def docker_calls(self):
        if not self.calls.exists():
            return []
        return [json.loads(line) for line in self.calls.read_text().splitlines()]

    def diagnostics(self):
        return sorted((self.runner / "backend-startup-logs").glob("startup.*"))

    def test_success_does_not_collect_diagnostics(self):
        result = self.run_up(status=0)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.diagnostics(), [])
        self.assertEqual(len(self.docker_calls()), 2)
        self.assertEqual(
            self.docker_calls()[-1][5:],
            ["up", "--detach", "--remove-orphans", "--wait", "backend", "replica"],
        )

    def test_failure_retains_logs_and_only_owned_container_state(self):
        result = self.run_up()
        self.assertEqual(result.returncode, 37, result.stderr)
        self.assertEqual(len(self.diagnostics()), 1, result.stderr)
        retained = self.diagnostics()[0]
        self.assertIn(str(retained), result.stderr)
        self.assertEqual((retained / "project.txt").read_text().strip(), PROJECT)
        self.assertIn("allocation_boundary", (retained / "compose.log").read_text())
        self.assertIn("Exited (37)", (retained / "compose-ps.txt").read_text())
        states = [
            json.loads(line)
            for line in (retained / "container-state.jsonl").read_text().splitlines()
        ]
        self.assertEqual(
            [state["name"] for state in states], ["owned-backend", "owned-db"]
        )
        self.assertEqual(states[0]["state"]["ExitCode"], 37)
        self.assertNotIn(
            "fixture-private-value", (retained / "container-state.jsonl").read_text()
        )
        for call in self.docker_calls():
            if call[0] == "compose":
                self.assertEqual(call[call.index("-p") + 1], PROJECT)
            elif call[0] == "inspect":
                self.assertEqual(call[1], "--format")
                self.assertIn(".State", call[2])
                self.assertNotIn(".Config", call[2])
                self.assertIn(call[-1], ["owned-backend", "owned-db"])
        self.assertEqual(retained.stat().st_mode & 0o777, 0o700)
        self.assertEqual((retained / "compose.log").stat().st_mode & 0o777, 0o600)

    def test_log_failure_still_collects_state_and_keeps_startup_status(self):
        result = self.run_up(failure="logs")
        self.assertEqual(result.returncode, 37, result.stderr)
        retained = self.diagnostics()[0]
        self.assertIn(
            "fixture log failure", (retained / "diagnostic-errors.log").read_text()
        )
        self.assertIn("owned-db", (retained / "container-state.jsonl").read_text())

    def test_inspect_failure_does_not_skip_other_owned_containers(self):
        result = self.run_up(failure="inspect")
        self.assertEqual(result.returncode, 37, result.stderr)
        retained = self.diagnostics()[0]
        self.assertIn(
            "fixture inspect failure", (retained / "diagnostic-errors.log").read_text()
        )
        states = [
            json.loads(line)
            for line in (retained / "container-state.jsonl").read_text().splitlines()
        ]
        self.assertEqual([state["name"] for state in states], ["owned-db"])

    def test_ps_failure_does_not_fall_back_to_global_inspection(self):
        result = self.run_up(failure="ps")
        self.assertEqual(result.returncode, 37, result.stderr)
        retained = self.diagnostics()[0]
        self.assertIn(
            "fixture ps failure", (retained / "diagnostic-errors.log").read_text()
        )
        self.assertFalse(any(call[0] == "inspect" for call in self.docker_calls()))

    def test_unwritable_log_parent_preserves_startup_status(self):
        occupied = self.directory / "occupied"
        occupied.touch()
        result = self.run_up(RUNNER_TEMP=str(occupied))
        self.assertEqual(result.returncode, 37, result.stderr)
        self.assertIn("Unable to create backend startup diagnostics", result.stderr)
        self.assertEqual(len(self.docker_calls()), 2)

    def test_repeated_failures_preserve_each_attempt(self):
        first = self.run_up(runner=False)
        self.assertEqual(first.returncode, 37, first.stderr)
        self.assertEqual(len(self.diagnostics()), 1, first.stderr)
        original = self.diagnostics()[0]
        second = self.run_up(status=42)
        self.assertEqual(second.returncode, 42, second.stderr)
        self.assertEqual(len(self.diagnostics()), 2)
        self.assertTrue((original / "compose.log").is_file())

    def test_worktree_refusal_does_not_start_or_inspect_containers(self):
        result = self.run_up(WORKTREE_STATUS="65")
        self.assertEqual(result.returncode, 65, result.stderr)
        self.assertEqual(self.docker_calls(), [])
        self.assertEqual(self.diagnostics(), [])


if __name__ == "__main__":
    unittest.main()
