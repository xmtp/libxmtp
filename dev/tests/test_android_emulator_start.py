#!/usr/bin/env python3
"""Exercise the real startup supervisor with executable emulator/ADB fixtures."""

import json
import os
from pathlib import Path
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
    }.get(mode)
    if mode == "segfault" or marker:
        if marker:
            while not (home / marker).exists():
                time.sleep(0.01)
        os.kill(os.getpid(), signal.SIGSEGV)
    while True:
        time.sleep(0.05)

if role == "clock":
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
        self.avd = self.home / "avd/libxmtp-test.avd"
        self.avd.mkdir(parents=True)
        (self.avd / "config.ini").write_text("hw.ramSize=4096\n")
        for role, code in [
            ("emulator", FIXTURE),
            ("adb", FIXTURE),
            ("clock", FIXTURE),
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
            ANDROID_USER_HOME=str(self.home),
            ANDROID_AVD_HOME=str(self.home / "avd"),
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

    def run_start(self, mode, interrupt=False):
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
                while not (self.home / "boot.started").exists():
                    if time.monotonic() >= deadline:
                        self.fail("Fixture did not reach boot polling")
                    time.sleep(0.02)
                process.terminate()
            stdout, stderr = process.communicate(timeout=8)
        except BaseException:
            process.kill()
            process.communicate()
            raise
        self.output = stdout + stderr
        self.record = json.loads((self.logs / "startup.json").read_text())
        return process.returncode

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
