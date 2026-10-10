#!/usr/bin/env python3
"""Check CREATE cancellation and uncertain outcomes against real PostgreSQL."""

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from metadata_backend import psql_connection


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--backend", required=True)
args = parser.parse_args()
psql = shutil.which("psql")
if psql is None:
    raise SystemExit("The Android Nix shell must provide psql.")
fixture = Path(__file__).with_name("metadata_backend.py")
database = os.environ["DATABASE_URL"]


def query(statement):
    with psql_connection(database, os.environ, psql) as (command, env):
        return subprocess.run(
            [*command, "-At", "-v", "ON_ERROR_STOP=1", "-c", statement],
            env=env,
            check=True,
            capture_output=True,
            text=True,
        )


def exists(name):
    assert name.startswith("messenger_metadata_") and len(name) == 51
    assert all(ch in "0123456789abcdef" for ch in name[19:])
    output = query(
        f"SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = '{name}')"
    )
    return output.stdout.strip() == "t"


with tempfile.TemporaryDirectory(prefix="metadata-create-proof-") as temporary:
    root = Path(temporary)
    wrapper = root / "psql"
    wrapper.write_text(
        f"#!{sys.executable}\n"
        + """import os,sys,subprocess,time
from pathlib import Path
command=sys.argv[-1]
status=subprocess.run([os.environ['REAL_PSQL'], *sys.argv[1:]]).returncode
if command.startswith('CREATE') and status == 0:
    Path(os.environ['CREATE_LEDGER']).write_text(command.split('"')[1])
    time.sleep(1)
    if os.environ.get('UNCERTAIN_CREATE'): status=8
sys.exit(status)
"""
    )
    wrapper.chmod(0o700)
    for signum in (signal.SIGINT, signal.SIGTERM, None):
        ledger = root / "created"
        ledger.unlink(missing_ok=True)
        env = dict(
            os.environ,
            REAL_PSQL=psql,
            CREATE_LEDGER=str(ledger),
            PATH=str(root) + os.pathsep + os.environ["PATH"],
        )
        if signum is None:
            env["UNCERTAIN_CREATE"] = "1"
        process = subprocess.Popen(
            [sys.executable, str(fixture), "--backend", args.backend, "--", "true"],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        name = None
        try:
            deadline = time.monotonic() + 15
            while not ledger.exists():
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise AssertionError(
                        "Real CREATE did not reach its committed boundary"
                    )
                time.sleep(0.01)
            name = ledger.read_text()
            assert exists(name), "The real database must exist before the signal"
            if signum is not None:
                process.send_signal(signum)
            out, error = process.communicate(timeout=40)
            expected = 128 + signum if signum is not None else 1
            assert process.returncode == expected, (process.returncode, out, error)
            assert not exists(name), "The exact owned database leaked after CREATE"
            print(
                json.dumps(
                    {
                        "signal": signum,
                        "name": name,
                        "exists_before": True,
                        "exists_after": False,
                        "status": process.returncode,
                    }
                ),
                flush=True,
            )
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=10)
            if name is not None and exists(name):
                query(f'DROP DATABASE "{name}" WITH (FORCE)')
