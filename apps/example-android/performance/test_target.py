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

    def test_active_private_lease_overrides_the_persistent_backend_environment(self):
        self.write()
        with patch.dict(os.environ, self.env, clear=True):
            self.assertEqual("http://127.0.0.1:45678", disposable_backend())

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


class DisposableRecipeTest(unittest.TestCase):
    def test_actual_recipe_uses_generated_fixture_route_and_preserves_fixed_shape(self):
        root = Path(__file__).resolve().parents[3]
        source = (root / "apps/example-android/example-android.just").read_text()
        line = (
            source.split('performance output="app/build/performance":', 1)[1]
            .splitlines()[1]
            .strip()
        )
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            fixture = home / "fixtures"
            performance = home / "performance"
            binaries = home / "bin"
            for path in (fixture, performance, binaries):
                path.mkdir(parents=True)
            (fixture / "metadata_backend.py").write_text("""import os, subprocess, sys
env = dict(os.environ, XMTP_METADATA_BACKEND_URL="http://127.0.0.1:45678", XMTP_METADATA_BACKEND_PORT="45678", XMTP_METADATA_BACKEND_LEASE="owned-lease")
raise SystemExit(subprocess.call(sys.argv[sys.argv.index("--")+1:], env=env))
""")
            (performance / "run.py").write_text("""import json, os, pathlib, sys
pathlib.Path(os.environ["ROUTE_RECORD"]).write_text(json.dumps({"args": sys.argv[1:], "url": os.environ.get("XMTP_METADATA_BACKEND_URL"), "lease": os.environ.get("XMTP_METADATA_BACKEND_LEASE"), "flags": os.environ.get("NIX_ANDROID_EMULATOR_FLAGS"), "api": os.environ.get("NIX_ANDROID_EMULATOR_API")}))
""")
            for name, body in {
                "run-test-emulator": 'import subprocess, sys\nraise SystemExit(subprocess.call(sys.argv[sys.argv.index("--")+1:]))\n',
                "adb": 'import json, os, pathlib, sys\npathlib.Path(os.environ["REVERSE_RECORD"]).write_text(json.dumps(sys.argv[1:]))\n',
            }.items():
                path = binaries / name
                path.write_text(f"#!{sys.executable}\n" + body)
                path.chmod(0o755)
            env = dict(
                os.environ,
                PATH=str(binaries) + os.pathsep + os.environ["PATH"],
                ANDROID_SERIAL="emulator-fixture",
                XMTP_BACKEND_URL="https://persistent.invalid",
                XMTP_BACKEND_PORT="5250",
                ROUTE_RECORD=str(home / "route.json"),
                REVERSE_RECORD=str(home / "reverse.json"),
            )
            command = (
                line.replace("{{ _env }}", "true")
                .replace("{{ root }}", directory)
                .replace("{{output}}", "result")
            )
            result = subprocess.run(
                ["bash", "-euc", command],
                cwd=home,
                env=env,
                text=True,
                capture_output=True,
            )
            self.assertEqual(0, result.returncode, result.stdout + result.stderr)
            route = json.loads((home / "route.json").read_text())
            reverse = json.loads((home / "reverse.json").read_text())
            self.assertEqual("http://127.0.0.1:45678", route["url"])
            self.assertEqual("owned-lease", route["lease"])
            self.assertNotIn("--backend", route["args"])
            self.assertEqual(
                ["-s", "emulator-fixture", "reverse", "tcp:45678", "tcp:45678"], reverse
            )
            self.assertEqual("34", route["api"])
            self.assertIn("-cores 4 -memory 4096", route["flags"])
            self.assertIn("--red-control", route["args"])


if __name__ == "__main__":
    unittest.main()
