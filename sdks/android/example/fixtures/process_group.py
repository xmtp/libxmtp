"""Retain one owned group identity until its final cleanup signal."""

import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time


class KeeperError(RuntimeError):
    pass


class OwnedProcess:
    def __init__(self, command, **options):
        if signal.getsignal(signal.SIGCHLD) != signal.SIG_DFL:
            raise KeeperError("Owned groups require normal child reaping.")
        self._keeper = None
        self._leased = False
        self._buffer = b""
        self._command_pid = None
        self._returncode = None
        self._error = None
        self._self_cleanup = False
        self._eof = False
        self._control = self._status = None
        control_read = status_write = None
        try:
            control_read, self._control = os.pipe()
            self._status, status_write = os.pipe()
            os.set_blocking(self._status, False)
            self._keeper = subprocess.Popen(
                [
                    sys.executable,
                    str(Path(__file__)),
                    "--keeper",
                    str(control_read),
                    str(status_write),
                    *command,
                ],
                start_new_session=True,
                pass_fds=(control_read, status_write),
                **options,
            )
            self._leased = True
        except BaseException:
            self._close_fds()
            raise
        finally:
            for descriptor in (control_read, status_write):
                if descriptor is not None:
                    os.close(descriptor)
        try:
            deadline = time.monotonic() + 5
            while self._command_pid is None:
                self._pump()
                if time.monotonic() >= deadline:
                    raise KeeperError(
                        "The owned keeper did not report command readiness."
                    )
                time.sleep(0.01)
        except BaseException:
            self.stop()
            raise

    @property
    def pid(self):
        return self._keeper.pid

    @property
    def command_pid(self):
        return self._command_pid

    @property
    def returncode(self):
        return self.poll()

    def _pump(self):
        if not self._leased:
            return
        if self._error is not None:
            raise KeeperError(self._error)
        while True:
            try:
                chunk = os.read(self._status, 4096)
            except BlockingIOError:
                break
            if not chunk:
                self._eof = True
                raise KeeperError("The owned keeper exited before group cleanup.")
            self._buffer += chunk
            while b"\n" in self._buffer:
                line, self._buffer = self._buffer.split(b"\n", 1)
                event = json.loads(line)
                if event.get("selfCleanup"):
                    self._self_cleanup = True
                    continue
                if "error" in event:
                    self._error = event["error"]
                    raise KeeperError(self._error)
                self._command_pid = event["pid"]
                self._returncode = event["returncode"]

    def poll(self):
        self._pump()
        return self._returncode

    def wait(self, timeout=None):
        deadline = None if timeout is None else time.monotonic() + timeout
        while self.poll() is None:
            if deadline is not None and time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired("owned fixture command", timeout)
            time.sleep(0.01)
        return self._returncode

    def _signal(self, signum):
        if not self._leased or self._keeper.returncode is not None:
            raise KeeperError("The owned process group lease has ended.")
        try:
            os.killpg(self.pid, signum)
        except ProcessLookupError:
            pass

    def _close_fds(self):
        for name in ("_control", "_status"):
            descriptor = getattr(self, name, None)
            if descriptor is not None:
                os.close(descriptor)
                setattr(self, name, None)

    def stop(self):
        if not self._leased:
            return
        try:
            if not (self._self_cleanup and self._eof):
                self._signal(signal.SIGTERM)
                try:
                    self.wait(timeout=10)
                except (subprocess.TimeoutExpired, KeeperError):
                    pass
                self._signal(signal.SIGKILL)
        finally:
            self._leased = False
            try:
                self._keeper.wait(timeout=5)
            finally:
                self._close_fds()


def keeper(control, status, command):
    # Caught signals reset to their default handlers when the command execs.
    for signum in (signal.SIGINT, signal.SIGTERM):
        signal.signal(signum, lambda *_: None)

    def report(value):
        os.write(status, (json.dumps(value) + "\n").encode())

    try:
        child = subprocess.Popen(command)
        report({"pid": child.pid, "returncode": None})
    except OSError:
        report({"error": "The fixture command could not start."})
        child = None
    reported = False
    while True:
        if child is not None and not reported:
            code = child.poll()
            if code is not None:
                report({"pid": child.pid, "returncode": code})
                reported = True
        ready, _, _ = select.select([control], [], [], 0.01)
        if ready and not os.read(control, 1):
            report({"selfCleanup": True})
            os.killpg(os.getpgrp(), signal.SIGKILL)


if __name__ == "__main__":
    if sys.argv[1] != "--keeper":
        raise SystemExit("The owned keeper is internal to the fixture.")
    keeper(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4:])
