#!/usr/bin/env python3
"""Run one app test command with an owned catalogue backend and database."""

import argparse
from contextlib import contextmanager
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


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def config_text(source, catalogue, listener, metrics):
    expected = tomllib.loads(source)
    expected["server"]["listen"] = f"127.0.0.1:{listener}"
    expected.setdefault("telemetry", {})["metrics_listen"] = f"127.0.0.1:{metrics}"
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


def run(backend, command, environment=None):
    env = dict(environment or os.environ)
    database = env["DATABASE_URL"]
    with psql_connection(database, env) as (psql, psql_env):
        return _run_owned(backend, command, env, database, psql, psql_env)


def _run_owned(backend, command, env, database, psql, psql_env):
    # This runner changes no shared database or service configuration.
    for name in ("XMTP_S3_URL", "XMTP_S3_BASE_URL"):
        if not env.get(name):
            raise ValueError(f"{name} is required from the worktree environment.")
    name = "messenger_metadata_" + uuid.uuid4().hex
    owned_url = owned_database_url(database, name)
    listener, metrics = port(), port()
    env.update(
        DATABASE_URL=owned_url, XMTP_DATABASE_URL=owned_url, XMTP_REPLICA_URL=owned_url
    )
    child_env = dict(
        env,
        XMTP_METADATA_BACKEND_URL=f"http://127.0.0.1:{listener}",
        XMTP_METADATA_BACKEND_PORT=str(listener),
    )
    env.setdefault("XMTP_CHAIN_31337_URL", env.get("ANVIL_URL", ""))
    env.pop("OTEL_EXPORTER_OTLP_ENDPOINT", None)
    fixture = Path(__file__).resolve().parent
    root = fixture.parents[3]
    create_attempted = False
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

    def database_exists():
        result = subprocess.run(
            [
                *psql,
                "-At",
                "-v",
                "ON_ERROR_STOP=1",
                "-c",
                f"SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = '{name}')",
            ],
            check=True,
            env=psql_env,
            timeout=30,
            text=True,
            capture_output=True,
        )
        return result.stdout.strip() == "t"

    for signum in (signal.SIGINT, signal.SIGTERM):
        previous[signum] = signal.signal(signum, interrupted)
    try:
        if database_exists():
            raise RuntimeError("The allocated fixture database already exists.")
        creating = True
        create_attempted = True
        try:
            subprocess.run(
                [
                    *psql,
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-c",
                    f'CREATE DATABASE "{name}"',
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
                    (root / "dev/backend/local-s3.toml").read_text(),
                    (fixture / "metadata-catalogue.toml").read_text(),
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
            print(f"Metadata fixture logs: {logs}", flush=True)
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
                            f"Metadata backend exited with {server.returncode}; see {logs}"
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
                            f"Metadata backend did not become ready; see {logs}"
                        )
                    time.sleep(0.1)
                lease = Path(directory) / "lease.json"
                with open(
                    lease, "x", opener=lambda path, flags: os.open(path, flags, 0o600)
                ) as out:
                    json.dump(
                        {
                            "formatVersion": 1,
                            "url": child_env["XMTP_METADATA_BACKEND_URL"],
                            "port": listener,
                            "database": name,
                            "serverPid": server.command_pid,
                            "groupPid": server.pid,
                            "ownerPid": os.getpid(),
                            "uid": os.getuid(),
                        },
                        out,
                    )
                child_env["XMTP_METADATA_BACKEND_LEASE"] = str(lease)
                child = OwnedProcess(command, env=child_env)
                return child.wait()
    finally:
        for signum in previous:
            signal.signal(signum, signal.SIG_IGN)
        try:
            stop(child)
            stop(server)
            if create_attempted and database_exists():
                subprocess.run(
                    [
                        *psql,
                        "-v",
                        "ON_ERROR_STOP=1",
                        "-c",
                        f'DROP DATABASE "{name}" WITH (FORCE)',
                    ],
                    check=True,
                    env=psql_env,
                    timeout=30,
                    stdout=subprocess.DEVNULL,
                )
        finally:
            for signum, handler in previous.items():
                signal.signal(signum, handler)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--backend", required=True, help="Path to the current xmtp-backend binary"
    )
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a test command is required")
    try:
        raise SystemExit(run(args.backend, command))
    except KeyboardInterrupt as error:
        raise SystemExit(128 + (error.args[0] if error.args else signal.SIGINT))
