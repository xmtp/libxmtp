"""Keep app termination available after the launcher process group is killed.

The iOS Simulator app is not in the launcher's process group. When the runner
kills that group at its outer timeout, the launcher cannot run its own
termination. The launcher registers the termination command here first, and
the runner runs it with its own deadline.
"""

import json
import subprocess
from pathlib import Path

from fixtures import canonical, digest

# Termination gets its own short deadline, apart from the operation deadline.
CLEANUP_SECONDS = 15


def registration_path(request):
    return Path(request["state_directory"]) / "ios-cleanup" / f"{digest(request)}.json"


def register(request, command, logs):
    path = registration_path(request)
    path.parent.mkdir(parents=True, exist_ok=True)
    value = {"command": command, "logs": str(logs.resolve())}
    temporary = path.with_suffix(".tmp")
    temporary.write_bytes(canonical(value))
    temporary.replace(path)


def clear(request):
    registration_path(request).unlink(missing_ok=True)


def terminate_registered(request):
    path = registration_path(request)
    if not path.exists():
        return
    registration = json.loads(path.read_text())
    command = registration["command"]
    record = {"argv": command, "reason": "outer-runner-timeout"}
    try:
        result = subprocess.run(
            command, capture_output=True, text=True, timeout=CLEANUP_SECONDS
        )
        record.update(
            returncode=result.returncode, stdout=result.stdout, stderr=result.stderr
        )
        if result.returncode and not (
            result.returncode == 3 and "found nothing to terminate" in result.stderr
        ):
            raise RuntimeError(f"iOS timeout cleanup failed: {result.stderr}")
        clear(request)
    except Exception as error:
        record["error"] = str(error)
        raise
    finally:
        logs = Path(registration["logs"])
        (logs / "runner-cleanup.json").write_bytes(canonical(record))
