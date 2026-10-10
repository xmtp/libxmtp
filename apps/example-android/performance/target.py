"""Admit only the active disposable backend lease created by the fixture."""

import json
import os
from pathlib import Path
import re
import stat
import subprocess
from urllib.parse import urlsplit


def disposable_backend():
    path = Path(os.environ.get("XMTP_METADATA_BACKEND_LEASE", ""))
    if not os.environ.get("XMTP_METADATA_BACKEND_LEASE"):
        raise ValueError("Run performance inside the owned disposable backend fixture")
    info = path.lstat()
    if (
        not stat.S_ISREG(info.st_mode)
        or info.st_uid != os.getuid()
        or stat.S_IMODE(info.st_mode) & 0o077
        or info.st_size > 16 * 1024
    ):
        raise ValueError("Disposable backend lease is not an owned private file")
    lease = json.loads(path.read_text())
    if not isinstance(lease, dict):
        raise ValueError("Invalid disposable backend lease")
    for key in ("port", "serverPid", "groupPid", "ownerPid", "uid"):
        if type(lease.get(key)) is not int or lease[key] <= 0 and key != "uid":
            raise ValueError("Invalid disposable backend lease")
    if (
        type(lease.get("formatVersion")) is not int
        or lease["formatVersion"] != 1
        or lease["uid"] != os.getuid()
    ):
        raise ValueError("Invalid disposable backend lease")
    if not 1 <= lease["port"] <= 65535:
        raise ValueError("Invalid disposable backend listener")
    backend = f"http://127.0.0.1:{lease['port']}"
    if (
        lease.get("url") != backend
        or os.environ.get("XMTP_METADATA_BACKEND_URL") != backend
        or os.environ.get("XMTP_METADATA_BACKEND_PORT") != str(lease["port"])
    ):
        raise ValueError("Backend target does not match its disposable lease")
    database = lease.get("database", "")
    if (
        not isinstance(database, str)
        or not re.fullmatch(r"messenger_metadata_[a-f0-9]{32}", database)
        or urlsplit(os.environ.get("DATABASE_URL", "")).path != "/" + database
    ):
        raise ValueError("Backend database is not the owned disposable database")
    for pid in (lease["serverPid"], lease["groupPid"], lease["ownerPid"]):
        os.kill(pid, 0)
    if (
        os.getpgid(lease["serverPid"]) != lease["groupPid"]
        or os.getsid(lease["serverPid"]) != lease["groupPid"]
        or os.getpgid(lease["groupPid"]) != lease["groupPid"]
    ):
        raise ValueError("Backend process does not belong to its owned session")
    ancestor = os.getppid()
    while ancestor > 1 and ancestor != lease["ownerPid"]:
        ancestor = int(
            subprocess.run(
                ["ps", "-o", "ppid=", "-p", str(ancestor)],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
        )
    if ancestor != lease["ownerPid"]:
        raise ValueError("Disposable backend owner is not the runner's ancestor")
    return backend
