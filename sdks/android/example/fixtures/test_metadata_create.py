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
import uuid

from metadata_backend import psql_connection


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--backend", required=True)
parser.add_argument(
    "--case",
    choices=(
        "signal-int",
        "signal-term",
        "uncertain-database",
        "uncertain-role",
        "database-collision",
        "role-collision",
        "late-database-replacement",
        "missing-role-permission",
    ),
)
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


def role_identity(name):
    assert name.startswith("messenger_metadata_")
    output = query(
        f"SELECT oid || ':' || COALESCE(shobj_description(oid, 'pg_authid'), '') "
        f"FROM pg_roles WHERE rolname = '{name}'"
    ).stdout.strip()
    return output or None


def database_identity(name):
    assert name.startswith("messenger_metadata_") and len(name) == 51
    return (
        query(
            f"SELECT oid || ':' || datdba FROM pg_database WHERE datname = '{name}'"
        ).stdout.strip()
        or None
    )


with tempfile.TemporaryDirectory(prefix="metadata-create-proof-") as temporary:
    root = Path(temporary)
    wrapper = root / "psql"
    wrapper.write_text(
        f"#!{sys.executable}\n"
        + """import os,sys,subprocess,time
from pathlib import Path
command=sys.argv[-1]
scenario=os.environ.get('CREATE_SCENARIO', '')
if os.environ.get('PERMISSION_ROLE'):
    command='SET ROLE "' + os.environ['PERMISSION_ROLE'] + '"; ' + command
    sys.exit(subprocess.run([os.environ['REAL_PSQL'], '-q', *sys.argv[1:-1], command]).returncode)
if scenario == 'late-database-replacement' and command.startswith('DROP DATABASE'):
    name=command.split('"')[1]
    prefix=[os.environ['REAL_PSQL'], *sys.argv[1:sys.argv.index('-c')]]
    subprocess.run([*prefix, '-c', 'DROP DATABASE "' + name + '" WITH (FORCE)'], check=True)
    subprocess.run([*prefix, '-c', 'CREATE DATABASE "' + name + '"'], check=True)
    identity=subprocess.run([*prefix, '-At', '-c', "SELECT oid || ':' || datdba FROM pg_database WHERE datname = '" + name + "'"], check=True, capture_output=True, text=True).stdout.strip()
    Path(os.environ['REPLACEMENT_LEDGER']).write_text(identity)
collision=(scenario == 'database-collision' and command.startswith('CREATE DATABASE')) or (scenario == 'role-collision' and command.startswith('CREATE ROLE'))
if collision:
    name=command.split('"')[1]
    statement=('CREATE DATABASE "' + name + '"') if command.startswith('CREATE DATABASE') else ('CREATE ROLE "' + name + '" NOLOGIN; COMMENT ON ROLE "' + name + '" IS ' + chr(39) + 'proof-foreign-generation' + chr(39))
    subprocess.run([os.environ['REAL_PSQL'], *sys.argv[1:-1], statement], check=True)
    Path(os.environ['CREATE_LEDGER']).write_text(name)
    time.sleep(1)
status=subprocess.run([os.environ['REAL_PSQL'], *sys.argv[1:]]).returncode
if command.startswith('CREATE ROLE') and status == 0:
    Path(os.environ['ROLE_LEDGER']).write_text(command.split('"')[1])
    if scenario == 'uncertain-role':
        Path(os.environ['CREATE_LEDGER']).write_text(command.split('"')[1])
        time.sleep(1)
        status=8
if command.startswith('CREATE DATABASE') and status == 0:
    Path(os.environ['CREATE_LEDGER']).write_text(command.split('"')[1])
    time.sleep(1)
    if os.environ.get('UNCERTAIN_CREATE'): status=8
sys.exit(status)
"""
    )
    wrapper.chmod(0o700)
    compile(wrapper.read_text(), str(wrapper), "exec")
    for scenario, signum in (
        ("signal-int", signal.SIGINT),
        ("signal-term", signal.SIGTERM),
        ("uncertain-database", None),
        ("uncertain-role", None),
        ("database-collision", None),
        ("role-collision", None),
        ("late-database-replacement", signal.SIGTERM),
    ):
        if args.case is not None and args.case != scenario:
            continue
        ledger = root / "created"
        ledger.unlink(missing_ok=True)
        role_ledger = root / "role"
        role_ledger.unlink(missing_ok=True)
        replacement_ledger = root / "replacement"
        replacement_ledger.unlink(missing_ok=True)
        env = dict(
            os.environ,
            REAL_PSQL=psql,
            CREATE_LEDGER=str(ledger),
            ROLE_LEDGER=str(role_ledger),
            CREATE_SCENARIO=scenario,
            REPLACEMENT_LEDGER=str(replacement_ledger),
            PATH=str(root) + os.pathsep + os.environ["PATH"],
        )
        if scenario == "uncertain-database":
            env["UNCERTAIN_CREATE"] = "1"
        process = subprocess.Popen(
            [sys.executable, str(fixture), "--backend", args.backend, "--", "true"],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        name = None
        role = None
        role_before = None
        database_before = None
        cleanup_database_identity = None
        try:
            deadline = time.monotonic() + 15
            while not ledger.exists():
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise AssertionError(
                        "Real CREATE did not reach its committed boundary"
                    )
                time.sleep(0.01)
            name = ledger.read_text()
            is_role = scenario in ("uncertain-role", "role-collision")
            if is_role:
                role = name
            else:
                role = role_ledger.read_text()
                assert exists(name), "The real database must exist before the signal"
                database_before = database_identity(name)
                cleanup_database_identity = database_before
            role_before = role_identity(role)
            assert role_before is not None, "The owned or collision role must exist"
            if signum is not None:
                process.send_signal(signum)
            out, error = process.communicate(timeout=40)
            expected = 128 + signum if signum is not None else 1
            assert process.returncode == expected, (process.returncode, out, error)
            if scenario == "database-collision":
                assert database_identity(name) == database_before, (
                    "The collision actor database changed"
                )
            elif scenario == "late-database-replacement":
                cleanup_database_identity = replacement_ledger.read_text()
                assert cleanup_database_identity != database_before
                assert database_identity(name) == cleanup_database_identity, (
                    "The late replacement database changed"
                )
            elif not is_role:
                assert not exists(name), "The exact owned database leaked after CREATE"
            role_after = role_identity(role)
            if scenario == "role-collision":
                assert role_after == role_before, "The foreign role changed"
            else:
                assert role_after is None, "The exact owned role leaked after CREATE"
            print(
                json.dumps(
                    {
                        "signal": signum,
                        "scenario": scenario,
                        "name": name,
                        "role": role,
                        "role_before": role_before,
                        "role_after": role_after,
                        "database_before": database_before,
                        "replacement_before": cleanup_database_identity
                        if scenario == "late-database-replacement"
                        else None,
                        "database_after_identity": None
                        if is_role
                        else database_identity(name),
                        "database_after": None if is_role else exists(name),
                        "status": process.returncode,
                    }
                ),
                flush=True,
            )
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=10)
            if (
                name is not None
                and cleanup_database_identity is not None
                and database_identity(name) == cleanup_database_identity
            ):
                query(f'DROP DATABASE "{name}" WITH (FORCE)')
            if (
                role is not None
                and role_before is not None
                and role_identity(role) == role_before
            ):
                query(f'DROP ROLE "{role}"')
            if name is not None and not is_role:
                assert not exists(name), "The proof database cleanup failed"
            if role is not None:
                assert role_identity(role) is None, "The proof role cleanup failed"

    if args.case not in (None, "missing-role-permission"):
        raise SystemExit(0)
    permission_role = "messenger_metadata_proof_" + uuid.uuid4().hex
    marker = uuid.uuid4().hex
    query(
        f'CREATE ROLE "{permission_role}" NOLOGIN NOCREATEDB NOCREATEROLE; '
        f"COMMENT ON ROLE \"{permission_role}\" IS '{marker}'"
    )
    identity = role_identity(permission_role)
    try:
        permission_env = dict(
            os.environ,
            REAL_PSQL=psql,
            PERMISSION_ROLE=permission_role,
            PATH=str(root) + os.pathsep + os.environ["PATH"],
        )
        result = subprocess.run(
            [sys.executable, str(fixture), "--backend", args.backend, "--", "true"],
            env=permission_env,
            capture_output=True,
            timeout=40,
        )
        assert result.returncode == 1
        assert b"needs CREATEDB and CREATEROLE" in result.stderr, result.stderr
        assert role_identity(permission_role) == identity
        print(
            json.dumps(
                {
                    "scenario": "missing-role-permission",
                    "status": 1,
                    "role_before": identity,
                    "role_after": identity,
                }
            ),
            flush=True,
        )
    finally:
        if role_identity(permission_role) == identity:
            query(f'DROP ROLE "{permission_role}"')
        assert role_identity(permission_role) is None
