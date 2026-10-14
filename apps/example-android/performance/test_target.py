import contextlib
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import run
from target import disposable_backend


class DisposableTargetTest(unittest.TestCase):
    def setUp(self):
        self.home = tempfile.TemporaryDirectory()
        self.addCleanup(self.home.cleanup)
        self.server = subprocess.Popen(
            [sys.executable, "-c", "import time;time.sleep(60)"], start_new_session=True
        )
        self.addCleanup(self.stop)
        self.path = Path(self.home.name) / "lease.json"
        self.lease = {
            "formatVersion": 1,
            "url": "http://127.0.0.1:45678",
            "port": 45678,
            "database": "messenger_metadata_" + "a" * 32,
            "serverPid": self.server.pid,
            "groupPid": self.server.pid,
            "ownerPid": os.getppid(),
            "uid": os.getuid(),
        }
        self.env = {
            "XMTP_METADATA_BACKEND_LEASE": str(self.path),
            "XMTP_METADATA_BACKEND_URL": self.lease["url"],
            "XMTP_METADATA_BACKEND_PORT": "45678",
            "DATABASE_URL": "postgres://fixture@127.0.0.1:5432/"
            + self.lease["database"],
            "XMTP_BACKEND_URL": "https://persistent.invalid",
        }

    def stop(self):
        if self.server.poll() is None:
            self.server.terminate()
        self.server.wait()

    def write(self, lease=None):
        self.path.write_text(json.dumps(lease or self.lease))
        self.path.chmod(0o600)

    def test_missing_or_non_disposable_lease_is_rejected(self):
        cases = [
            {},
            {"url": "https://persistent.invalid"},
            {"database": "production"},
            {"ownerPid": self.server.pid},
            {"groupPid": os.getpgrp()},
            {"port": 0},
        ]
        for changes in cases:
            with self.subTest(changes=changes):
                lease = copy.deepcopy(self.lease)
                lease.update(changes)
                self.write(lease)
                env = dict(self.env)
                if not changes:
                    env.pop("XMTP_METADATA_BACKEND_LEASE")
                with patch.dict(os.environ, env, clear=True):
                    with self.assertRaises(
                        (ValueError, OSError, subprocess.CalledProcessError)
                    ):
                        disposable_backend()

    def test_unowned_or_stale_lease_is_rejected(self):
        self.write()
        self.path.chmod(0o644)
        with patch.dict(os.environ, self.env, clear=True):
            with self.assertRaisesRegex(ValueError, "owned private file"):
                disposable_backend()
            self.path.chmod(0o600)
            self.stop()
            with self.assertRaises(ProcessLookupError):
                disposable_backend()

    def test_runner_rejects_arbitrary_backend_cli_and_admits_before_execution(self):
        with (
            patch.object(
                sys,
                "argv",
                [
                    "run.py",
                    "--output",
                    self.home.name,
                    "--backend",
                    "https://persistent.invalid",
                ],
            ),
            contextlib.redirect_stderr(__import__("io").StringIO()),
        ):
            with self.assertRaises(SystemExit) as failure:
                run.main()
            self.assertEqual(2, failure.exception.code)
        with (
            patch.dict(os.environ, {}, clear=True),
            patch.object(run, "execute") as execute,
        ):
            with self.assertRaisesRegex(ValueError, "owned disposable backend"):
                run.run(Path(self.home.name), False)
            execute.assert_not_called()


if __name__ == "__main__":
    unittest.main()
