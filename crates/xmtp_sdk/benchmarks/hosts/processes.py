"""Run a host process and record the peak RSS of its process tree."""

import os
import signal
import subprocess
import sys
from threading import Event, Thread, Timer
import time

# ru_maxrss is in bytes on macOS and in KiB on Linux.
MAXRSS_UNIT = 1 if sys.platform == "darwin" else 1024


def execute(argv, input_text=None, timeout=None):
    """Return (exit code, stdout, stderr, duration ms, peak RSS bytes).

    A sampler sums RSS over the child and its descendants every 10 ms. The
    child's own high-water mark from wait4 is a floor, so a child that exits
    before the sampler runs still reports its memory.
    """
    child = subprocess.Popen(
        argv,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    peak = [0]
    stopped = Event()
    errors = []
    output = {}

    def sample_once():
        result = subprocess.run(
            ["ps", "-axo", "pid=,ppid=,rss="],
            text=True,
            capture_output=True,
            check=True,
        )
        rows = [
            tuple(map(int, line.split()))
            for line in result.stdout.splitlines()
            if line.strip()
        ]
        tree = {child.pid}
        previous = set()
        while previous != tree:
            previous = tree.copy()
            tree.update(pid for pid, parent, _ in rows if parent in tree)
        total = sum(rss * 1024 for pid, _, rss in rows if pid in tree)
        peak[0] = max(peak[0], total)

    def sample():
        try:
            while not stopped.wait(0.01):
                sample_once()
        except Exception as error:
            errors.append(error)

    def read(name, stream):
        with stream:
            output[name] = stream.read()

    def kill():
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass

    timed_out = Event()

    def expire():
        timed_out.set()
        kill()

    start = time.perf_counter()
    try:
        sample_once()
    except Exception:
        kill()
        child.communicate()
        raise
    threads = [
        Thread(target=sample, daemon=True),
        Thread(target=read, args=("stdout", child.stdout), daemon=True),
        Thread(target=read, args=("stderr", child.stderr), daemon=True),
    ]
    for thread in threads:
        thread.start()
    timer = Timer(timeout, expire) if timeout else None
    if timer:
        timer.start()
    try:
        try:
            child.stdin.write(input_text or "")
            child.stdin.close()
        except BrokenPipeError:
            pass
        _, status, usage = os.wait4(child.pid, 0)
        child.returncode = os.waitstatus_to_exitcode(status)
    finally:
        if timer:
            timer.cancel()
        stopped.set()
        for thread in threads:
            thread.join()
    duration = (time.perf_counter() - start) * 1000
    if timed_out.is_set():
        raise TimeoutError(f"Host exceeded {timeout} s: {argv}")
    if errors:
        raise RuntimeError("Process-tree memory sampling failed") from errors[0]
    peak[0] = max(peak[0], usage.ru_maxrss * MAXRSS_UNIT)
    return child.returncode, output["stdout"], output["stderr"], duration, peak[0]
