"""Collect RSS for an executable and all of its descendant processes."""

import subprocess
import threading
import time


def execute(argv, input_text=None):
    child = subprocess.Popen(
        argv,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    peak = [0]
    stopped = threading.Event()
    errors = []

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
        parents = {child.pid}
        previous = set()
        while previous != parents:
            previous = parents.copy()
            parents.update(pid for pid, parent, _ in rows if parent in parents)
        peak[0] = max(
            peak[0], sum(rss * 1024 for pid, _, rss in rows if pid in parents)
        )

    def sample():
        try:
            while not stopped.wait(0.01):
                sample_once()
        except Exception as error:
            errors.append(error)

    # Sample before the background thread can be delayed past child completion.
    start = time.perf_counter()
    try:
        sample_once()
    except Exception:
        child.kill()
        child.communicate()
        raise
    thread = threading.Thread(target=sample, daemon=True)
    thread.start()
    try:
        stdout, stderr = child.communicate(input_text)
    finally:
        stopped.set()
        thread.join()
    duration = (time.perf_counter() - start) * 1000
    if errors:
        raise RuntimeError("Process-tree memory sampling failed") from errors[0]
    if peak[0] <= 0:
        raise RuntimeError("Process-tree memory could not be sampled")
    return child.returncode, stdout, stderr, duration, peak[0]
