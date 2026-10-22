#!/usr/bin/env python3
"""Run one app test command with an owned catalogue backend and database."""

import argparse
from contextlib import contextmanager
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import tempfile
import time
import tomllib
from urllib.parse import unquote, urlsplit, urlunsplit
import uuid

from process_group import OwnedProcess


@dataclass(frozen=True)
class Fixture:
    """One owned backend: its base config, catalogue, and exported names."""

    label: str
    database_prefix: str
    base_config: str
    catalogue: str | None
    url_variable: str
    port_variable: str
    lease_variable: str | None
    needs_s3: bool


# The catalogue backend that metadata tests use, with attachment storage.
METADATA = Fixture(
    label="Metadata",
    database_prefix="messenger_metadata_",
    base_config="dev/backend/local-s3.toml",
    catalogue="metadata-catalogue.toml",
    url_variable="XMTP_METADATA_BACKEND_URL",
    port_variable="XMTP_METADATA_BACKEND_PORT",
    lease_variable="XMTP_METADATA_BACKEND_LEASE",
    needs_s3=True,
)


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def config_text(source, catalogue, listener, metrics):
    expected = tomllib.loads(source)
    expected["server"]["listen"] = f"127.0.0.1:{listener}"
    expected.setdefault("telemetry", {})["metrics_listen"] = f"127.0.0.1:{metrics}"
    if catalogue:
        expected["application_components"] = tomllib.loads(catalogue)[
            "application_components"
        ]
    text = re.sub(r"(?m)^listen\s*=.*$", f'listen = "127.0.0.1:{listener}"', source)
    if "[telemetry]" in text:
        raise ValueError(
            "The fixture base already has telemetry. Update the fixture derivation."
        )
    text += f'\n[telemetry]\nmetrics_listen = "127.0.0.1:{metrics}"\n' + catalogue
    if tomllib.loads(text) != expected:
        raise ValueError("Unexpected fixture configuration change.")
    return text


def stop(process):
    if process is None:
        return
    if not isinstance(process, OwnedProcess):
        raise ValueError("The fixture process does not own a retained group lease.")
    process.stop()


@contextmanager
def psql_connection(database, environment, executable="psql"):
    parsed = urlsplit(database)
    if parsed.scheme not in ("postgres", "postgresql"):
        raise ValueError("DATABASE_URL must be a PostgreSQL URI.")
    password = unquote(parsed.password) if parsed.password is not None else None
    query = []
    for parameter in parsed.query.split("&"):
        key, separator, value = parameter.partition("=")
        if unquote(key) == "password":
            if not separator:
                raise ValueError("The PostgreSQL password parameter needs a value.")
            password = unquote(value)
        else:
            query.append(parameter)
    authority = parsed.netloc
    if parsed.password is not None:
        user, host = authority.rsplit("@", 1)
        authority = user.split(":", 1)[0] + "@" + host
    if password is not None:
        query = [
            item for item in query if unquote(item.partition("=")[0]) != "passfile"
        ]
    connection = urlunsplit(parsed._replace(netloc=authority, query="&".join(query)))
    with tempfile.TemporaryDirectory(prefix="metadata-psql-") as directory:
        env = dict(environment)
        for name in ("DATABASE_URL", "XMTP_DATABASE_URL", "XMTP_REPLICA_URL"):
            env.pop(name, None)
        if password is not None:
            if "\n" in password or "\r" in password:
                raise ValueError("A PostgreSQL passfile cannot contain a line break.")
            passfile = Path(directory) / "password"
            with open(
                passfile, "x", opener=lambda path, flags: os.open(path, flags, 0o600)
            ) as out:
                out.write(
                    "*:*:*:*:"
                    + password.replace("\\", "\\\\").replace(":", "\\:")
                    + "\n"
                )
            env["PGPASSFILE"] = str(passfile)
            env.pop("PGPASSWORD", None)
        yield [executable, "--no-password", connection], env


def owned_database_url(database, name):
    parsed = urlsplit(database)
    query = "&".join(
        parameter
        for parameter in parsed.query.split("&")
        if unquote(parameter.partition("=")[0]) != "dbname"
    )
    return urlunsplit(parsed._replace(path="/" + name, query=query))


def run(backend, command, environment=None, fixture=METADATA):
    env = dict(environment or os.environ)
    database = env["DATABASE_URL"]
    with psql_connection(database, env) as (psql, psql_env):
        return _run_owned(backend, command, env, database, psql, psql_env, fixture)


def _run_owned(backend, command, env, database, psql, psql_env, fixture=METADATA):
    # This runner changes no shared database or service configuration.
    for name in ("XMTP_S3_URL", "XMTP_S3_BASE_URL") if fixture.needs_s3 else ():
        if not env.get(name):
            raise ValueError(f"{name} is required from the worktree environment.")
    name = fixture.database_prefix + uuid.uuid4().hex
    owner = fixture.database_prefix + "owner_" + uuid.uuid4().hex
    generation = uuid.uuid4().hex
    owned_url = owned_database_url(database, name)
    listener, metrics = port(), port()
    env.update(
        DATABASE_URL=owned_url, XMTP_DATABASE_URL=owned_url, XMTP_REPLICA_URL=owned_url
    )
    child_env = dict(env)
    child_env[fixture.url_variable] = f"http://127.0.0.1:{listener}"
    child_env[fixture.port_variable] = str(listener)
    env.setdefault("XMTP_CHAIN_31337_URL", env.get("ANVIL_URL", ""))
    env.pop("OTEL_EXPORTER_OTLP_ENDPOINT", None)
    fixtures = Path(__file__).resolve().parent
    root = fixtures.parents[2]
    create_attempted = False
    owner_oid = None
    owner_attempted = False
    creating = False
    pending_signal = None
    server = child = None
    previous = {}

    def interrupted(signum, _frame):
        nonlocal pending_signal
        if creating:
            pending_signal = signum
        else:
            raise KeyboardInterrupt(signum)

    def query(statement):
        result = subprocess.run(
            [
                *psql,
                "-At",
                "-v",
                "ON_ERROR_STOP=1",
                "-c",
                statement,
            ],
            check=True,
            env=psql_env,
            timeout=30,
            text=True,
            capture_output=True,
        )
        return result.stdout.strip()

    def database_exists():
        return (
            query(f"SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = '{name}')")
            == "t"
        )

    def owned_role_oid():
        value = query(
            f"SELECT oid FROM pg_roles WHERE rolname = '{owner}' "
            f"AND shobj_description(oid, 'pg_authid') = '{generation}'"
        )
        return int(value) if value else None

    def owns_database():
        return (
            owner_oid is not None
            and query(
                f"SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = '{name}' "
                f"AND datdba = {owner_oid})"
            )
            == "t"
        )

    for signum in (signal.SIGINT, signal.SIGTERM):
        previous[signum] = signal.signal(signum, interrupted)
    try:
        if (
            query(
                "SELECT rolsuper OR (rolcreaterole AND rolcreatedb) "
                "FROM pg_roles WHERE rolname = CURRENT_USER"
            )
            != "t"
        ):
            raise RuntimeError("The metadata fixture needs CREATEDB and CREATEROLE.")
        if database_exists():
            raise RuntimeError("The allocated fixture database already exists.")
        creating = True
        owner_attempted = True
        try:
            # One transaction binds the private role to this generation.
            subprocess.run(
                [
                    *psql,
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-c",
                    f'CREATE ROLE "{owner}" NOLOGIN; '
                    f"COMMENT ON ROLE \"{owner}\" IS '{generation}'; "
                    f'GRANT "{owner}" TO CURRENT_USER',
                ],
                check=True,
                env=psql_env,
                timeout=30,
                stdout=subprocess.DEVNULL,
                start_new_session=True,
            )
            owner_oid = owned_role_oid()
            if owner_oid is None:
                raise RuntimeError("The fixture role ownership is not established.")
            create_attempted = True
            subprocess.run(
                [
                    *psql,
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-c",
                    f'CREATE DATABASE "{name}" OWNER "{owner}"',
                ],
                check=True,
                env=psql_env,
                timeout=30,
                stdout=subprocess.DEVNULL,
                start_new_session=True,
            )
        finally:
            creating = False
        if pending_signal is not None:
            raise KeyboardInterrupt(pending_signal)
        with tempfile.TemporaryDirectory(prefix="messenger-metadata-") as directory:
            config = Path(directory) / "backend.toml"
            config.write_text(
                config_text(
                    (root / fixture.base_config).read_text(),
                    (fixtures / fixture.catalogue).read_text()
                    if fixture.catalogue
                    else "",
                    listener,
                    metrics,
                )
            )
            logs = Path(
                env["XMTP_METADATA_LOG_DIR"]
                if env.get("XMTP_METADATA_LOG_DIR")
                else tempfile.mkdtemp(
                    prefix="messenger-metadata-logs-",
                    dir=env.get("RUNNER_TEMP", "/tmp"),
                )
            )
            logs.mkdir(parents=True, exist_ok=True)
            print(f"{fixture.label} fixture logs: {logs}", flush=True)
            with (logs / "backend.log").open("w") as log:
                server = OwnedProcess(
                    [backend, "--config-file", str(config)],
                    env=env,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                )
                deadline = time.monotonic() + 60
                while True:
                    if server.poll() is not None:
                        raise RuntimeError(
                            f"{fixture.label} backend exited with {server.returncode}; see {logs}"
                        )
                    probe = subprocess.run(
                        [
                            "grpc-health-probe",
                            "-addr",
                            f"127.0.0.1:{listener}",
                            "-connect-timeout",
                            "1s",
                            "-rpc-timeout",
                            "1s",
                        ],
                        stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL,
                        timeout=5,
                    )
                    if probe.returncode == 0:
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError(
                            f"{fixture.label} backend did not become ready; see {logs}"
                        )
                    time.sleep(0.1)
                print(
                    f"Owned {fixture.label.lower()} backend ready: "
                    + child_env[fixture.url_variable],
                    flush=True,
                )
                if fixture.lease_variable:
                    lease = Path(directory) / "lease.json"
                    with open(
                        lease,
                        "x",
                        opener=lambda path, flags: os.open(path, flags, 0o600),
                    ) as out:
                        json.dump(
                            {
                                "formatVersion": 1,
                                "url": child_env[fixture.url_variable],
                                "port": listener,
                                "database": name,
                                "serverPid": server.command_pid,
                                "groupPid": server.pid,
                                "ownerPid": os.getpid(),
                                "uid": os.getuid(),
                            },
                            out,
                        )
                    child_env[fixture.lease_variable] = str(lease)
                child = OwnedProcess(command, env=child_env)
                return child.wait()
    finally:
        for signum in previous:
            signal.signal(signum, signal.SIG_IGN)
        try:
            stop(child)
            stop(server)
            if owner_attempted:
                retained_oid = owned_role_oid()
                if owner_oid is None:
                    owner_oid = retained_oid
                elif retained_oid != owner_oid:
                    raise RuntimeError("The fixture role identity changed.")
            if create_attempted and owns_database():
                try:
                    subprocess.run(
                        [
                            *psql,
                            "-v",
                            "ON_ERROR_STOP=1",
                            "-c",
                            f'SET ROLE "{owner}"',
                            "-c",
                            f'DROP DATABASE "{name}" WITH (FORCE)',
                        ],
                        check=True,
                        env=psql_env,
                        timeout=30,
                        stdout=subprocess.DEVNULL,
                    )
                except subprocess.CalledProcessError:
                    # The server can reject a foreign replacement after admission.
                    if owns_database():
                        raise
            if owner_oid is not None:
                if owned_role_oid() != owner_oid or owns_database():
                    raise RuntimeError("The fixture role is not safe to remove.")
                # DROP ROLE fails closed if another database still owns it.
                subprocess.run(
                    [*psql, "-v", "ON_ERROR_STOP=1", "-c", f'DROP ROLE "{owner}"'],
                    check=True,
                    env=psql_env,
                    timeout=30,
                    stdout=subprocess.DEVNULL,
                )
        finally:
            for signum, handler in previous.items():
                signal.signal(signum, handler)


def main(fixture=METADATA, description=__doc__):
    parser = argparse.ArgumentParser(description=description)
    parser.add_argument(
        "--backend", required=True, help="Path to the current xmtp-backend binary"
    )
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a test command is required")
    try:
        raise SystemExit(run(args.backend, command, fixture=fixture))
    except KeyboardInterrupt as error:
        raise SystemExit(128 + (error.args[0] if error.args else signal.SIGINT))


if __name__ == "__main__":
    main()
