"""Check lane setup and result detection without a live sbx VM."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest


RUNNER = Path(__file__).resolve().parents[2] / ".agents/skills/security-hardening/scripts/guest_lanes.py"
FAKE = """#!/usr/bin/env python3
import json, pathlib, sys
prompt = sys.stdin.read()
pathlib.Path('received-prompt').write_text(prompt)
if pathlib.Path(sys.argv[0]).name == 'claude':
    print(json.dumps({'type': 'result', 'is_error': False, 'result': 'complete'}))
else:
    print(json.dumps({'type': 'thread.started', 'thread_id': 'test-thread'}))
    print(json.dumps({'type': 'turn.failed', 'error': 'test failure'}))
"""


class GuestLaneTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="security-lanes-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git("init", "-q")
        (self.repo / "file.txt").write_text("audited code\n")
        self.git("add", "file.txt")
        self.git("-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "base")
        self.base = self.git("rev-parse", "HEAD").stdout.strip()
        self.run_dir = self.root / "run"
        (self.run_dir / "prompts").mkdir(parents=True)
        (self.run_dir / "prompts" / "a.md").write_text("audit A\n")
        (self.run_dir / "prompts" / "b.md").write_text("audit B\n")
        (self.run_dir / "manifest.json").write_text(json.dumps({
            "repo": str(self.repo),
            "base": self.base,
            "concurrency": 2,
            "lanes": [
                {"id": "claude-a", "cli": "claude", "model": "opus", "effort": "high", "prompt": "prompts/a.md"},
                {"id": "codex-b", "cli": "codex", "model": "gpt-6", "effort": "high", "prompt": "prompts/b.md"},
            ],
        }))
        binary = self.root / "bin"
        binary.mkdir()
        for cli in ("claude", "codex"):
            executable = binary / cli
            executable.write_text(FAKE)
            executable.chmod(0o755)
        self.env = {**os.environ, "PATH": f"{binary}:{os.environ['PATH']}"}

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], text=True, capture_output=True, check=True)

    def tearDown(self):
        for path in (self.run_dir / "supervisor.pid", *self.run_dir.glob("lanes/*/pid")):
            if not path.exists():
                continue
            try:
                os.kill(int(path.read_text()), 15)
            except ProcessLookupError:
                pass

    def runner(self, action):
        return subprocess.run(
            [sys.executable, str(RUNNER), action, str(self.run_dir)],
            text=True, capture_output=True, env=self.env,
        )

    def test_parallel_lanes_have_isolated_worktrees_and_real_final_states(self):
        prepared = self.runner("init")
        self.assertEqual(prepared.returncode, 0, prepared.stderr)
        started = self.runner("start")
        self.assertEqual(started.returncode, 0, started.stderr)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            results = [
                self.run_dir / "lanes" / lane / "result.json"
                for lane in ("claude-a", "codex-b")
            ]
            if all(path.exists() for path in results):
                break
            time.sleep(0.2)
        else:
            self.fail("lane results did not arrive")
        self.assertEqual(json.loads(results[0].read_text())["state"], "done")
        self.assertEqual(json.loads(results[1].read_text())["state"], "failed")
        self.assertEqual(json.loads(results[1].read_text())["session_id"], "test-thread")
        for lane, prompt in (("claude-a", "audit A\n"), ("codex-b", "audit B\n")):
            worktree = self.run_dir / "worktrees" / lane
            self.assertEqual((worktree / "received-prompt").read_text(), prompt)
            head = subprocess.run(
                ["git", "-C", str(worktree), "rev-parse", "HEAD"],
                text=True, capture_output=True, check=True,
            ).stdout.strip()
            self.assertEqual(head, self.base)
        status = self.runner("status")
        self.assertEqual(json.loads(status.stdout), {"claude-a": "done", "codex-b": "failed"})
        supervisor_pid = int((self.run_dir / "supervisor.pid").read_text())
        while time.monotonic() < deadline:
            try:
                os.kill(supervisor_pid, 0)
            except ProcessLookupError:
                break
            time.sleep(0.2)


if __name__ == "__main__":
    unittest.main()
