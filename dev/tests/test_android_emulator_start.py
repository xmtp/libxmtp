#!/usr/bin/env python3
"""Exercise the real startup supervisor with executable emulator/ADB fixtures."""

import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = Path(
    os.environ.get(
        "ANDROID_EMULATOR_START_HELPER", ROOT / "nix/lib/android-emulator-start.py"
    )
)
CLOCK_HELPER = Path(
    os.environ.get("ANDROID_CLOCK_HELPER", ROOT / "nix/lib/android-sync-clock.sh")
)
FIXTURE = r"""
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys
import time

home = Path(os.environ["FIXTURE_HOME"])
mode = os.environ["FIXTURE_MODE"]
role = Path(sys.argv[0]).name
args = sys.argv[1:]
if role == "emulator" and args in (["-version"], ["-accel-check"]):
    print("emulator diagnostic " + args[0])
    sys.exit(0)
if role == "adb" and args[:1] == ["devices"]:
    print("List of devices attached")
    sys.exit(0)
if role == "adb" and "logcat" in args:
    if mode == "hung-diagnostics":
        time.sleep(60)
    print("fixture guest log")
    sys.exit(0)

(home / f"{role}-{os.getpid()}.pid").touch()
if role == "emulator":
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    crashdb = home / "emu-crash-fixture.db"
    (crashdb / "pending").mkdir(parents=True)
    (crashdb / "pending/.fixture.dmp").write_text("fixture minidump")
    print(f"Storing crashdata in: {crashdb}, detection is enabled", flush=True)
    print("fixture kernel output", flush=True)
    if mode == "exit":
        sys.exit(7)
    marker = {
        "crash-adb": "adb.started",
        "crash-boot": "boot.started",
        "crash-clock": "clock.started",
        "crash-clock-real": "clock.started",
        "crash-test": "test.started",
    }.get(mode)
    if mode == "segfault" or marker:
        if marker:
            while not (home / marker).exists():
                time.sleep(0.01)
        os.kill(os.getpid(), signal.SIGSEGV)
    while True:
        time.sleep(0.05)

if role == "test":
    import json
    (home / "test.env").write_text(json.dumps({key: os.environ[key] for key in (
        "ANDROID_SERIAL", "ANDROID_USER_HOME", "ANDROID_AVD_HOME"
    )}))
    subprocess.Popen([sys.executable, str(home / "child")])
    while not (home / "child.started").exists():
        time.sleep(0.01)
    (home / "test.started").touch()
    if mode in ("test-hang", "crash-test"):
        time.sleep(60)
    if mode == "long-test":
        time.sleep(2.2)
    sys.exit(7 if mode == "test-failure" else 0)
elif role == "clock":
    (home / "clock.started").touch()
    if mode == "clock-failure":
        print("clock differs from the host", flush=True)
        sys.exit(9)
    if mode in ("crash-clock", "hung-clock"):
        time.sleep(60)
    print("clock synchronized")
elif args[2:] == ["root"] and mode == "crash-clock-real":
    subprocess.Popen([sys.executable, str(home / "child")])
    while not (home / "child.started").exists():
        time.sleep(0.01)
    (home / "clock.started").touch()
    time.sleep(60)
elif args[2:] == ["get-state"]:
    (home / "adb.started").touch()
    # These scenarios must not report an online device before emulator death.
    if mode in ("exit", "segfault"):
        time.sleep(60)
    if mode in ("crash-adb", "hung-adb"):
        child = subprocess.Popen([sys.executable, str(home / "child")])
        while not (home / "child.started").exists():
            time.sleep(0.01)
        time.sleep(60)
    if mode == "offline":
        sys.exit(1)
    print("device\r")
elif args[2:] == ["shell", "getprop", "dev.bootcomplete"]:
    (home / "boot.started").touch()
    counter = home / "boot.counter"
    count = int(counter.read_text()) + 1 if counter.exists() else 1
    counter.write_text(str(count))
    if mode == "crash-boot":
        time.sleep(60)
    print("10" if mode in ("boot-hang", "hung-diagnostics") else ("0" if count < 3 else "1\r"))
elif args[2:] == ["shell", "getprop", "ro.build.version.sdk"]:
    print("34" if mode == "wrong-api" else "23\r")
else:
    raise SystemExit("Unexpected command: " + repr(args))
"""
CHILD = r"""
import os
from pathlib import Path
import signal
import time
home = Path(os.environ["FIXTURE_HOME"])
signal.signal(signal.SIGTERM, signal.SIG_IGN)
(home / f"child-{os.getpid()}.pid").touch()
(home / "child.started").touch()
while True:
    time.sleep(0.05)
"""


class EmulatorStartupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name)
        self.logs = self.home / "logs"
        self.android_home = self.home / "nix-android-user-home-fixture"
        self.avd = self.android_home / "avd/libxmtp-test.avd"
        self.avd.mkdir(parents=True)
        (self.avd / "config.ini").write_text("hw.ramSize=4096\n")
        for role, code in [
            ("emulator", FIXTURE),
            ("adb", FIXTURE),
            ("clock", FIXTURE),
            ("test", FIXTURE),
            ("child", CHILD),
        ]:
            executable = self.home / role
            executable.write_text(f"#!{sys.executable}\n" + code)
            executable.chmod(0o755)
        clock = self.home / "clock.sh"
        clock.write_text(f'#!/usr/bin/env bash\nexec "{self.home / "clock"}"\n')
        # Shorten only production budgets; run the same CLI and subprocess logic.
        helper = self.home / "supervisor.py"
        helper.write_text(
            HELPER.read_text()
            .replace("STARTUP_TIMEOUT = 300", "STARTUP_TIMEOUT = 2")
            .replace("COMMAND_TIMEOUT = 15", "COMMAND_TIMEOUT = 1")
            .replace("POLL_INTERVAL = 0.25", "POLL_INTERVAL = 0.02")
            .replace("CLEANUP_TIMEOUT = 2", "CLEANUP_TIMEOUT = 0.2")
            .replace("DIAGNOSTIC_TIMEOUT = 15", "DIAGNOSTIC_TIMEOUT = 1")
        )
        self.command = [
            sys.executable,
            str(helper),
            str(self.home / "adb"),
            str(self.home / "emulator"),
            "libxmtp-test",
            "emulator-5560",
            "23",
            str(clock),
            "-no-window",
        ]
        self.env = dict(
            os.environ,
            FIXTURE_HOME=str(self.home),
            ANDROID_USER_HOME=str(self.android_home),
            ANDROID_AVD_HOME=str(self.android_home / "avd"),
            NIX_ANDROID_EMULATOR_LOG_DIR=str(self.logs),
        )
        self.addCleanup(self.clean_processes)

    def processes(self):
        return [
            (path.name.split("-")[0], int(path.stem.split("-")[1]))
            for path in self.home.glob("*.pid")
        ]

    @staticmethod
    def alive(pid):
        try:
            os.kill(pid, 0)
            # Orphaned zombies can briefly await init's reap on Linux.
            status = Path(f"/proc/{pid}/stat")
            return not status.exists() or status.read_text().split(") ")[1][0] != "Z"
        except ProcessLookupError:
            return False

    def clean_processes(self):
        for _, pid in self.processes():
            if self.alive(pid):
                try:
                    os.killpg(pid, signal.SIGKILL)
                except ProcessLookupError:
                    os.kill(pid, signal.SIGKILL)

    def run_start(
        self, mode, interrupt=False, marker="boot.started", signum=signal.SIGTERM
    ):
        process = subprocess.Popen(
            self.command,
            env=dict(self.env, FIXTURE_MODE=mode),
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            if interrupt:
                deadline = time.monotonic() + 3
                while not (self.home / marker).exists():
                    if time.monotonic() >= deadline:
                        self.fail(f"Fixture did not reach {marker}")
                    time.sleep(0.02)
                process.send_signal(signum)
            stdout, stderr = process.communicate(timeout=8)
        except BaseException:
            process.kill()
            process.communicate()
            raise
        self.output = stdout + stderr
        if "NIX_ANDROID_EMULATOR_LOG_DIR" not in self.env:
            self.logs = Path(
                re.search(r"Emulator startup diagnostics: (.+)", self.output)[1]
            )
            self.addCleanup(shutil.rmtree, self.logs)
        self.record = json.loads((self.logs / "startup.json").read_text())
        return process.returncode

    def scope_command(self):
        self.command.extend(["--", str(self.android_home), str(self.home / "test")])

    def assert_scope_cleaned(self):
        self.assertFalse(self.android_home.exists(), "Owned Android home survived")
        self.assertTrue(self.logs.exists(), "Retained diagnostics were removed")
        for role, pid in self.processes():
            self.assertFalse(self.alive(pid), f"Owned {role} process {pid} survived")

    def test_scoped_success_stops_emulator_and_test_descendant_and_removes_home(self):
        self.scope_command()
        self.assertEqual(self.run_start("success"), 0, self.output)
        self.assertEqual(self.record["status"], "completed")
        self.assertEqual(self.record["test_exit_status"], 0)
        self.assert_scope_cleaned()
        env = json.loads((self.home / "test.env").read_text())
        self.assertEqual(env["ANDROID_SERIAL"], "emulator-5560")
        self.assertEqual(env["ANDROID_USER_HOME"], str(self.android_home))
        self.assertEqual(env["ANDROID_AVD_HOME"], str(self.android_home / "avd"))

    def test_scoped_failure_preserves_test_exit_status_and_diagnostics(self):
        self.scope_command()
        self.assertEqual(self.run_start("test-failure"), 7, self.output)
        self.assertEqual(self.record["status"], "test failed")
        self.assertEqual(self.record["test_exit_status"], 7)
        self.assertIn("fixture guest log", (self.logs / "logcat.txt").read_text())
        self.assert_scope_cleaned()

    def test_scoped_sigterm_stops_all_owned_processes_and_removes_home(self):
        self.scope_command()
        self.assertEqual(
            self.run_start("test-hang", interrupt=True, marker="test.started"),
            143,
            self.output,
        )
        self.assert_scope_cleaned()

    def test_scoped_sigint_stops_all_owned_processes_and_removes_home(self):
        self.scope_command()
        self.assertEqual(
            self.run_start(
                "test-hang", interrupt=True, marker="test.started", signum=signal.SIGINT
            ),
            130,
            self.output,
        )
        self.assert_scope_cleaned()

    def test_scoped_startup_crash_removes_home_without_running_tests(self):
        self.scope_command()
        self.assert_failed("segfault", "signal 11 (SIGSEGV)", "ADB connection")
        self.assertFalse((self.home / "test.started").exists())
        self.assert_scope_cleaned()

    def test_scoped_emulator_crash_during_tests_stops_blocked_test(self):
        self.scope_command()
        self.assertEqual(self.run_start("crash-test"), 1, self.output)
        self.assertIn(
            "Emulator exited during test command: signal 11 (SIGSEGV)", self.output
        )
        self.assertEqual(self.record["phase"], "test command")
        self.assertEqual(self.record["emulator_exit_status"], -signal.SIGSEGV)
        self.assert_scope_cleaned()

    def test_scoped_tests_can_outlast_startup_deadline(self):
        self.scope_command()
        self.assertEqual(self.run_start("long-test"), 0, self.output)
        self.assertLess(self.record["startup_seconds"], 2)
        self.assertGreater(self.record["elapsed_seconds"], 2)
        self.assert_scope_cleaned()

    def test_scoped_run_preserves_unrelated_emulator_and_home(self):
        other_home = self.home / "other-android-home"
        other_home.mkdir()
        other = subprocess.Popen(
            [str(self.home / "emulator")],
            env=dict(self.env, FIXTURE_HOME=str(other_home), FIXTURE_MODE="success"),
            start_new_session=True,
            stdout=subprocess.DEVNULL,
        )

        def stop_other():
            other.terminate()
            other.wait(timeout=2)

        self.addCleanup(stop_other)
        self.scope_command()
        self.assertEqual(self.run_start("success"), 0, self.output)
        self.assert_scope_cleaned()
        self.assertIsNone(other.poll(), "Unrelated emulator was killed")
        self.assertTrue(other_home.exists(), "Unrelated home was removed")

    def test_scoped_default_diagnostics_survive_home_removal(self):
        self.env.pop("NIX_ANDROID_EMULATOR_LOG_DIR")
        self.scope_command()
        self.assertEqual(self.run_start("test-failure"), 7, self.output)
        self.assertFalse(self.logs.is_relative_to(self.android_home))
        self.assert_scope_cleaned()

    def assert_failed(self, mode, reason, phase):
        self.assertNotEqual(self.run_start(mode), 0, self.output)
        self.assertIn(reason, self.output)
        self.assertNotIn("Emulator ready", self.output)
        self.assertEqual(self.record["status"], "failed")
        self.assertEqual(self.record["phase"], phase)
        self.assertLess(self.record["elapsed_seconds"], 6)
        self.assertEqual((self.logs / "config.ini").read_text(), "hw.ramSize=4096\n")
        for role, pid in self.processes():
            self.assertFalse(
                self.alive(pid), f"{role} process {pid} survived startup failure"
            )

    def test_success_waits_for_boot_and_preserves_live_emulator(self):
        self.assertEqual(self.run_start("success"), 0, self.output)
        self.assertIn("Emulator ready (emulator-5560)", self.output)
        self.assertEqual(self.record["status"], "ready")
        self.assertTrue(self.alive(self.record["pid"]))
        self.assertEqual((self.home / "boot.counter").read_text(), "3")
        self.assertTrue((self.home / "clock.started").exists())
        self.assertEqual(self.record["command"][-1], "-no-window")

    def test_exit_before_adb_fails_and_retains_diagnostics(self):
        self.assert_failed("exit", "exit status 7", "ADB connection")
        self.assertIn("fixture kernel output", self.output)
        self.assertIn(
            "emulator diagnostic -version", (self.logs / "version.txt").read_text()
        )
        self.assertEqual(
            (self.logs / "crashdb/pending/.fixture.dmp").read_text(), "fixture minidump"
        )

    def test_sigsegv_before_adb_is_reported(self):
        self.assert_failed("segfault", "signal 11 (SIGSEGV)", "ADB connection")
        self.assertEqual(self.record["emulator_exit_status"], -signal.SIGSEGV)

    def test_crash_while_adb_is_blocked_stops_adb_and_descendant(self):
        self.assert_failed("crash-adb", "signal 11 (SIGSEGV)", "ADB connection")

    def test_crash_while_boot_property_is_blocked(self):
        self.assert_failed("crash-boot", "signal 11 (SIGSEGV)", "boot completion")

    def test_crash_during_clock_sync(self):
        self.assert_failed(
            "crash-clock", "signal 11 (SIGSEGV)", "clock synchronization"
        )

    def test_crash_during_real_clock_helper_kills_timeout_and_adb(self):
        self.command[7] = str(CLOCK_HELPER)
        self.assert_failed(
            "crash-clock-real", "signal 11 (SIGSEGV)", "clock synchronization"
        )
        self.assertTrue((self.home / "child.started").exists())

    def test_offline_device_has_a_deadline(self):
        self.assert_failed("offline", "startup deadline exceeded", "ADB connection")

    def test_boot_requires_exact_completion_property(self):
        self.assert_failed("boot-hang", "startup deadline exceeded", "boot completion")
        self.assertFalse((self.home / "clock.started").exists())

    def test_hung_adb_is_bounded_and_its_descendant_is_killed(self):
        self.assert_failed("hung-adb", "Startup command timed out", "ADB connection")
        self.assertTrue((self.home / "child.started").exists())

    def test_hung_clock_uses_the_shared_startup_deadline(self):
        self.assert_failed(
            "hung-clock", "startup deadline exceeded", "clock synchronization"
        )

    def test_clock_failure_prevents_readiness(self):
        self.assert_failed(
            "clock-failure", "clock synchronization failed", "clock synchronization"
        )

    def test_api_mismatch_prevents_readiness(self):
        self.assert_failed(
            "wrong-api", "API mismatch: expected 23, got 34", "API verification"
        )

    def test_hung_diagnostics_cannot_hold_failure_open(self):
        self.assert_failed(
            "hung-diagnostics", "startup deadline exceeded", "boot completion"
        )
        self.assertIn("deadline exceeded", (self.logs / "logcat.txt").read_text())

    def test_sigterm_cleans_up_startup(self):
        self.assertNotEqual(self.run_start("boot-hang", interrupt=True), 0, self.output)
        self.assertIn("Startup interrupted", self.output)
        self.assertTrue(all(not self.alive(pid) for _, pid in self.processes()))


if __name__ == "__main__":
    unittest.main()
