#!/usr/bin/env python3
"""Run one app test command with an owned catalogue backend and database."""

import argparse
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import tempfile
import time
import tomllib
from urllib.parse import urlsplit, urlunsplit
import uuid


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
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)


def run(backend, command, environment=None):
    env = dict(environment or os.environ)
    database = env["DATABASE_URL"]
    # This runner changes no shared database or service configuration.
    for name in ("XMTP_S3_URL", "XMTP_S3_BASE_URL"):
        if not env.get(name):
            raise ValueError(f"{name} is required from the worktree environment.")
    name = "messenger_metadata_" + uuid.uuid4().hex
    parsed = urlsplit(database)
    owned_url = urlunsplit(parsed._replace(path="/" + name))
    listener, metrics = port(), port()
    child_env = dict(
        env,
        XMTP_METADATA_BACKEND_URL=f"http://127.0.0.1:{listener}",
        XMTP_METADATA_BACKEND_PORT=str(listener),
    )
    env.update(
        DATABASE_URL=owned_url, XMTP_DATABASE_URL=owned_url, XMTP_REPLICA_URL=owned_url
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
                "psql",
                database,
                "-At",
                "-v",
                "ON_ERROR_STOP=1",
                "-c",
                f"SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = '{name}')",
            ],
            check=True,
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
                    "psql",
                    database,
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-c",
                    f'CREATE DATABASE "{name}"',
                ],
                check=True,
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
                server = subprocess.Popen(
                    [backend, "--config-file", str(config)],
                    env=env,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
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
                child = subprocess.Popen(command, env=child_env, start_new_session=True)
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
                        "psql",
                        database,
                        "-v",
                        "ON_ERROR_STOP=1",
                        "-c",
                        f'DROP DATABASE "{name}" WITH (FORCE)',
                    ],
                    check=True,
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
