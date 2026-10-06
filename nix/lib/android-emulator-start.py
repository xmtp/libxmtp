#!/usr/bin/env python3
"""Supervise cold boot without leaving CI waiting on a dead emulator."""

import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

STARTUP_TIMEOUT = 300
COMMAND_TIMEOUT = 15
POLL_INTERVAL = 0.25
CLEANUP_TIMEOUT = 2
DIAGNOSTIC_TIMEOUT = 15


class StartupFailure(Exception):
    pass


class StartupInterrupted(KeyboardInterrupt):
    def __init__(self, signum):
        super().__init__("Startup interrupted")
        self.exit_status = 128 + signum


def stop_process(process):
    """Reap the child and kill descendants, including a stuck adb shell."""

    def send(sig):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            pass
        except PermissionError as error:
            # Darwin returns EPERM for a group whose leader is exiting or reaped.
            # While the leader exits, waitpid can still report it as running,
            # so give it the cleanup budget to finish. A live child must still
            # be terminated or report the error.
            try:
                process.wait(timeout=CLEANUP_TIMEOUT)
            except subprocess.TimeoutExpired:
                raise error from None

    send(signal.SIGTERM)
    try:
        process.wait(timeout=CLEANUP_TIMEOUT)
    except subprocess.TimeoutExpired:
        pass
    # The group can outlive its leader when a descendant ignores SIGTERM.
    send(signal.SIGKILL)
    process.wait(timeout=CLEANUP_TIMEOUT)


def check_emulator(emulator, phase="before readiness"):
    status = emulator.poll()
    if status is not None:
        detail = (
            f"signal {-status} ({signal.Signals(-status).name})"
            if status < 0
            else f"exit status {status}"
        )
        raise StartupFailure(f"Emulator exited {phase}: {detail}")


def run_command(args, deadline, emulator=None, timeout=COMMAND_TIMEOUT, env=None):
    """Bound commands and also notice emulator death while ADB is blocked."""
    if emulator is not None:
        check_emulator(emulator)
    if time.monotonic() >= deadline:
        raise StartupFailure("Emulator startup deadline exceeded")
    command_deadline = min(deadline, time.monotonic() + timeout)
    with tempfile.TemporaryFile() as output:
        process = subprocess.Popen(
            args,
            stdin=subprocess.DEVNULL,
            stdout=output,
            stderr=subprocess.STDOUT,
            start_new_session=True,
            env=env,
        )
        try:
            while True:
                if emulator is not None:
                    check_emulator(emulator)
                if time.monotonic() >= deadline:
                    raise StartupFailure("Emulator startup deadline exceeded")
                if process.poll() is not None:
                    output.seek(0)
                    return process.returncode, output.read().decode(errors="replace")
                if time.monotonic() >= command_deadline:
                    raise StartupFailure(f"Startup command timed out: {' '.join(args)}")
                time.sleep(POLL_INTERVAL)
        finally:
            stop_process(process)


def diagnostics(directory, adb, binary, serial, emulator_pid):
    # Share one budget: collecting diagnostics must not become another hang.
    deadline = time.monotonic() + DIAGNOSTIC_TIMEOUT
    commands = [
        ("version.txt", [binary, "-version"]),
        ("acceleration.txt", [binary, "-accel-check"]),
        ("devices.txt", [adb, "devices", "-l"]),
        ("logcat.txt", [adb, "-s", serial, "logcat", "-d", "-t", "200"]),
    ]
    if emulator_pid is not None and shutil.which("coredumpctl"):
        commands.append(
            ("core.txt", ["coredumpctl", "--no-pager", "info", str(emulator_pid)])
        )
    for name, command in commands:
        try:
            env = None
            if name == "core.txt":
                # Host systemd tools must use host libraries, not the emulator's
                # Nix libraries (which can require a newer glibc ABI).
                env = dict(os.environ)
                env.pop("LD_LIBRARY_PATH", None)
            status, output = run_command(command, deadline, timeout=3, env=env)
            output = f"exit status: {status}\n{output}"
        except (StartupFailure, OSError) as error:
            output = str(error)
        (directory / name).write_text(output)


def retain_crash_data(directory, emulator_log):
    # Crashpad's location is printed by the emulator and varies with the host.
    for path in re.findall(
        r"Storing crashdata in: (.+?/emu-crash-[^,\n]+\.db)", emulator_log
    ):
        source = Path(path)
        if source.is_dir():
            shutil.copytree(source, directory / "crashdb", dirs_exist_ok=True)


def start(adb, binary, avd, serial, api, clock_helper, flags, test_command=None):
    if "NIX_ANDROID_EMULATOR_LOG_DIR" in os.environ:
        directory = Path(os.environ["NIX_ANDROID_EMULATOR_LOG_DIR"])
    elif test_command is not None:
        # Keep diagnostics outside the temporary Android home that we remove.
        directory = Path(tempfile.mkdtemp(prefix="nix-android-emulator-logs-"))
    else:
        directory = Path(os.environ["ANDROID_USER_HOME"]) / "startup"
    directory.mkdir(parents=True, exist_ok=True)
    avd_directory = Path(os.environ["ANDROID_AVD_HOME"]) / f"{avd}.avd"
    command = [
        binary,
        "-avd",
        avd,
        "-no-boot-anim",
        "-port",
        serial.split("-")[-1],
        *flags,
    ]
    record = {
        "command": command,
        "api": api,
        "serial": serial,
        "avd": str(avd_directory),
        "android_home": os.environ["ANDROID_USER_HOME"],
        "host": platform.platform(),
        "status": "starting",
    }
    for name in ("cpuinfo", "meminfo"):
        path = Path("/proc") / name
        if path.exists():
            (directory / f"{name}.txt").write_bytes(path.read_bytes())
    kvm = Path("/dev/kvm")
    record["kvm_accessible"] = kvm.exists() and os.access(kvm, os.R_OK | os.W_OK)
    record["free_disk_bytes"] = shutil.disk_usage(avd_directory).free
    emulator = None
    test_process = None
    ready = False
    began = time.monotonic()
    deadline = began + STARTUP_TIMEOUT
    print(f"Emulator startup diagnostics: {directory}", file=sys.stderr, flush=True)
    try:
        with (directory / "emulator.log").open("w") as output:
            emulator = subprocess.Popen(
                command,
                stdin=subprocess.DEVNULL,
                stdout=output,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        record["pid"] = emulator.pid
        phase = "ADB connection"
        while True:
            record["phase"] = phase
            status, output = run_command(
                [adb, "-s", serial, "get-state"], deadline, emulator
            )
            record["last_adb_output"] = output
            if status == 0 and output.strip() == "device":
                break
            time.sleep(POLL_INTERVAL)
        phase = "boot completion"
        while True:
            record["phase"] = phase
            status, output = run_command(
                [adb, "-s", serial, "shell", "getprop", "dev.bootcomplete"],
                deadline,
                emulator,
            )
            record["last_adb_output"] = output
            if status == 0 and output.strip() == "1":
                break
            time.sleep(POLL_INTERVAL)
        record["phase"] = "clock synchronization"
        status, output = run_command(
            ["bash", clock_helper, adb, serial], deadline, emulator, STARTUP_TIMEOUT
        )
        print(output, end="", file=sys.stderr)
        if status != 0:
            raise StartupFailure(
                f"Android clock synchronization failed (exit status {status})"
            )
        record["phase"] = "API verification"
        status, output = run_command(
            [adb, "-s", serial, "shell", "getprop", "ro.build.version.sdk"],
            deadline,
            emulator,
        )
        if status != 0 or output.strip() != api:
            raise StartupFailure(
                f"Android emulator API mismatch: expected {api}, got {output.strip()}"
            )
        check_emulator(emulator)
        ready = True
        record["status"] = "ready"
        record["startup_seconds"] = time.monotonic() - began
        print(f"Emulator ready ({serial})", file=sys.stderr)
        if test_command is not None:
            record["phase"] = "test command"
            record["test_command"] = test_command
            test_process = subprocess.Popen(
                test_command,
                env=dict(os.environ, ANDROID_SERIAL=serial),
                start_new_session=True,
            )
            # Tests have their own CI timeout; the startup deadline ends at ready.
            while test_process.poll() is None:
                check_emulator(emulator, "during test command")
                time.sleep(POLL_INTERVAL)
            status = test_process.returncode
            if status == 0:
                check_emulator(emulator, "during test command")
            record["test_exit_status"] = status
            record["status"] = "completed" if status == 0 else "test failed"
            return 128 - status if status < 0 else status
    except (StartupFailure, OSError, KeyboardInterrupt) as error:
        record["status"] = "failed"
        record["error"] = str(error)
        print(
            f"Android emulator startup failed during {record.get('phase', 'launch')}: {error}",
            file=sys.stderr,
        )
        return error.exit_status if isinstance(error, StartupInterrupted) else 1
    finally:
        # Finish owned-process teardown even if cancellation arrives again.
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        if test_process is not None:
            stop_process(test_process)
        failed = record["status"] not in ("ready", "completed")
        if failed:
            try:
                diagnostics(
                    directory,
                    adb,
                    binary,
                    serial,
                    emulator.pid if emulator is not None else None,
                )
            except OSError as error:
                print(
                    f"Could not collect emulator diagnostics: {error}", file=sys.stderr
                )
        if emulator is not None and (not ready or test_command is not None):
            stop_process(emulator)
            record["emulator_exit_status"] = emulator.poll()
        if failed:
            log = directory / "emulator.log"
            emulator_log = log.read_text(errors="replace") if log.exists() else ""
            print(emulator_log[-16000:], end="", file=sys.stderr)
            try:
                retain_crash_data(directory, emulator_log)
            except OSError as error:
                print(f"Could not retain emulator crash data: {error}", file=sys.stderr)
        for name in ("config.ini", "hardware-qemu.ini"):
            source = avd_directory / name
            if source.exists():
                shutil.copyfile(source, directory / name)
        record["elapsed_seconds"] = time.monotonic() - began
        (directory / "startup.json").write_text(json.dumps(record, indent=2) + "\n")
    return 0


def interrupted(signum, _frame):
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    raise StartupInterrupted(signum)


def main(args):
    flags = args[6:]
    owned_home = None
    test_command = None
    if "--" in flags:
        separator = flags.index("--")
        owned_home = Path(flags[separator + 1])
        test_command = flags[separator + 2 :]
        flags = flags[:separator]
        # The launcher passes only the home it just created, never a device PID
        # or a previously existing user's Android home.
        if owned_home != Path(os.environ["ANDROID_USER_HOME"]) or not test_command:
            raise ValueError("Expected the launcher-owned Android home and a command")
    try:
        return start(*args[:6], flags, test_command)
    finally:
        if owned_home is not None:
            try:
                shutil.rmtree(owned_home)
            except FileNotFoundError:
                pass
            print(f"Removed owned Android home: {owned_home}", file=sys.stderr)


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    sys.exit(main(sys.argv[1:]))
