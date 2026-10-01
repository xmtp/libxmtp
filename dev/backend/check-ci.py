#!/usr/bin/env python3
"""Check the wrapper's process contract with controlled service processes.

The services bind real sockets. They do not prove PostgreSQL, S3, or backend
compatibility; the native attachment contract and Swift jobs prove those.
All fault controls apply to a private wrapper copy, never to the public app.
"""

import argparse
import os
import re
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
SOURCE = (ROOT / "dev/backend/ci").read_text()
PORTS = (5050, 9464, 55432, 9067)

STUB = r"""import json, os, select, signal, socket, stat, subprocess, sys, time, tomllib
from pathlib import Path
root = Path(__file__).resolve().parent
mode = (root / "mode").read_text()
name = Path(sys.argv[0]).name
with (root / "pids").open("a") as out:
    out.write(str(os.getpid()) + "\n")
def connect(port):
    try:
        with socket.create_connection(("127.0.0.1", port), .1): pass
        return True
    except OSError: return False
def listen(ports):
    sockets = []
    for port in ports:
        s = socket.socket()
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        address = "127.0.0.1"
        if (mode == "backend-wildcard" and port == 5050) or (
            mode == "metrics-wildcard" and port == 9464
        ): address = "0.0.0.0"
        s.bind((address, port)); s.listen()
        sockets.append(s)
    (root / (name + "-started")).touch()
    while True:
        readable, _, _ = select.select(sockets, [], [], .1)
        for listener in readable:
            connection, _ = listener.accept()
            with connection:
                if listener.getsockname()[1] == 9464:
                    connection.settimeout(1)
                    request = connection.recv(4096)
                    if request:
                        status = "500 Error" if mode == "metrics-error" else "200 OK"
                        connection.sendall(f"HTTP/1.1 {status}\r\nContent-Length: 2\r\n\r\nok".encode())
if name == "initdb":
    Path(sys.argv[sys.argv.index("-D") + 1]).mkdir()
    if mode == "setup-term":
        (root / "setup-started").touch()
        while True: time.sleep(.1)
elif name == "postgres": listen([55432])
elif name == "versitygw": listen([9067])
elif name == "pg_isready": sys.exit(0 if connect(55432) else 1)
elif name == "createdb": sys.exit(0 if connect(55432) else 1)
elif name == "aws":
    assert os.environ["AWS_ACCESS_KEY_ID"] == "xmtps3"
    assert "AWS_PROFILE" not in os.environ and "HTTPS_PROXY" not in os.environ
    assert sys.argv[sys.argv.index("--endpoint-url") + 1] == "http://127.0.0.1:9067"
    if "list-buckets" in sys.argv: sys.exit(0 if connect(9067) else 1)
    if "put-bucket-policy" in sys.argv:
        json.loads(Path(sys.argv[sys.argv.index("--policy") + 1][7:]).read_text())
    print("S3 setup", sys.argv, flush=True)
elif name == "backend":
    config = Path(sys.argv[sys.argv.index("--config-file") + 1])
    assert config.parent.parent == Path(os.environ["RUNNER_TEMP"])
    assert config.parent.name.startswith("backend-ci.")
    assert stat.S_IMODE(config.stat().st_mode) == 0o600
    assert stat.S_IMODE(config.parent.stat().st_mode) == 0o700
    expected = tomllib.loads((root / "source-config.toml").read_text())
    expected["server"]["listen"] = "127.0.0.1:5050"
    expected.setdefault("telemetry", {})["metrics_listen"] = "127.0.0.1:9464"
    assert tomllib.loads(config.read_text()) == expected, "derived config changed other fields"
    (root / "config-path").write_text(str(config))
    if mode == "backend-exit": sys.exit(42)
    if mode == "listener-owner":
        subprocess.Popen([sys.executable, str(root / "other-listener")])
        while True: time.sleep(.1)
    listen([5050, 9464])
elif name == "other-listener": listen([5050, 9464])
elif name == "grpc-health-probe":
    sys.exit(0 if mode != "unhealthy" and connect(5050) else 1)
"""

CHILD = r"""import json, os, signal, subprocess, sys, time
from pathlib import Path
root = Path(sys.argv[1])
assert sys.argv[2:] == ["one argument", "", "$literal; no shell"]
expected = {
    "XMTP_BACKEND_URL": "http://127.0.0.1:5050",
    "DATABASE_URL": "postgres://xmtp:xmtp@127.0.0.1:55432/xmtp_backend",
    "XMTP_S3_URL": "http://127.0.0.1:9067",
    "XMTP_S3_BASE_URL": "http://127.0.0.1:9067/attachments",
    "NIX_DEVSHELL": "ios",
}
for key, value in expected.items(): assert os.environ[key] == value, key
assert "OTEL_EXPORTER_OTLP_ENDPOINT" not in os.environ
(root / "child-started").touch()
mode = (root / "mode").read_text()
if mode in ("child-term", "child-int", "success"):
    descendant = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(300)"])
    (root / "descendant").write_text(str(descendant.pid))
if mode in ("child-term", "child-int"):
    while True: time.sleep(.1)
sys.exit(23 if mode == "exit23" else 0)
"""


def wait_file(path, process):
    deadline = time.monotonic() + 6
    while not path.exists():
        assert process.poll() is None, f"wrapper exited before {path.name}"
        assert time.monotonic() < deadline, f"missing {path.name}"
        time.sleep(0.02)


def process_alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def check(mode, mutation=None):
    with tempfile.TemporaryDirectory(prefix="backend-ci-check-") as directory:
        root = Path(directory)
        # Use private ports when the developer has the local Docker stack up.
        # The installed wrapper still has the four fixed CI ports.
        reservations = [socket.socket() for _ in PORTS]
        for sock in reservations:
            sock.bind(("127.0.0.1", 0))
        ports = tuple(sock.getsockname()[1] for sock in reservations)
        for sock in reservations:
            sock.close()

        def private_ports(text):
            mapping = dict(zip(map(str, PORTS), map(str, ports)))
            return re.sub(r"\b(5050|9464|55432|9067)\b", lambda m: mapping[m[0]], text)

        run = root / "run"
        run.mkdir()
        tools = root / "tools"
        tools.mkdir()
        (tools / "python3").symlink_to(sys.executable)
        (tools / "mode").write_text(mode)
        stub = tools / "stub"
        stub.write_text(f"#!{sys.executable}\n" + private_ports(STUB))
        stub.chmod(0o755)
        for name in (
            "initdb",
            "postgres",
            "versitygw",
            "pg_isready",
            "createdb",
            "aws",
            "backend",
            "grpc-health-probe",
            "other-listener",
        ):
            (tools / name).symlink_to(stub)
        child = tools / "child.py"
        child.write_text(private_ports(CHILD))
        policy = root / "policy.json"
        policy.write_text("invalid" if mode == "bucket-failure" else "{}")
        shared = (ROOT / "dev/backend/local-s3.toml").read_bytes()
        config_text = shared.decode()
        if mode == "telemetry":
            config_text += (
                '\n[telemetry]\nmetrics_listen = "0.0.0.0:9464"\nsample_ratio = 0.5\n'
            )
        config = tools / "source-config.toml"
        config.write_text(private_ports(config_text))
        config_before = config.read_bytes()
        source = SOURCE.replace("sleep 120;", "sleep 5;")
        for key, value in {
            "tools": tools,
            "backend": tools / "backend",
            "config": config,
            "policy": policy,
            "cors": ROOT / "dev/docker/s3/cors.json",
        }.items():
            source = source.replace(f"@{key}@", str(value))
        if mutation:
            old, new = mutation
            assert old in source, f"mutation target absent: {old}"
            source = source.replace(old, new)
        wrapper = root / "ci"
        wrapper.write_text(private_ports(source))
        env = dict(
            os.environ,
            RUNNER_TEMP=str(run),
            NIX_DEVSHELL="ios",
            AWS_PROFILE="must-not-be-read",
            HTTPS_PROXY="http://invalid:1",
            OTEL_EXPORTER_OTLP_ENDPOINT="http://invalid:2",
        )
        sentinel = None
        if mode == "occupied":
            sentinel = socket.socket()
            sentinel.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            sentinel.bind(("127.0.0.1", ports[0]))
            sentinel.listen()
        output = root / "output"
        started = time.monotonic()
        with output.open("w") as log:
            process = subprocess.Popen(
                [
                    shutil.which("bash"),
                    str(wrapper),
                    sys.executable,
                    str(child),
                    str(tools),
                    "one argument",
                    "",
                    "$literal; no shell",
                ],
                env=env,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            try:
                if mode == "setup-term":
                    wait_file(tools / "setup-started", process)
                    derived = list(run.glob("backend-ci.*/config.toml"))
                    assert len(derived) == 1, "private config missing during setup"
                    assert derived[0].stat().st_mode & 0o777 == 0o600
                    process.send_signal(signal.SIGTERM)
                elif mode in ("child-term", "child-int"):
                    wait_file(tools / "descendant", process)
                    process.send_signal(
                        signal.SIGTERM if mode == "child-term" else signal.SIGINT
                    )
                try:
                    status = process.wait(
                        timeout=28 if "term" in mode or "int" in mode else 8
                    )
                except subprocess.TimeoutExpired:
                    raise AssertionError(
                        "startup deadline or cleanup limit failed"
                    ) from None
                duration = time.monotonic() - started
                text = output.read_text()
                expected = {
                    "success": 0,
                    "telemetry": 0,
                    "exit23": 23,
                    "setup-term": 143,
                    "child-term": 143,
                    "child-int": 130,
                }.get(mode)
                if expected is not None:
                    assert status == expected, (
                        f"exit {status}, expected {expected}\n{text}\n"
                        + (run / "backend-ci-logs/backend.log").read_text()
                    )
                else:
                    assert status != 0, "unexpected success"
                    assert not (tools / "child-started").exists(), (
                        "test child ran before setup was ready"
                    )
                if mode == "occupied":
                    assert f"port {ports[0]} is unavailable" in text, text
                    assert not (tools / "postgres-started").exists(), (
                        "services started despite occupied port"
                    )
                    with socket.create_connection(sentinel.getsockname(), 0.1):
                        pass
                    sentinel.close()
                    sentinel = None
                if mode == "unhealthy":
                    assert "startup exceeded 120 seconds" in text, text
                if mode == "backend-exit":
                    assert "service exited during startup" in text, text
                if mode in ("backend-wildcard", "metrics-wildcard"):
                    assert "Unexpected backend listeners" in text, text
                if mode == "listener-owner":
                    assert "CalledProcessError" in text, text
                if mode == "metrics-error":
                    assert "HTTP Error 500" in text, text
                assert config.read_bytes() == config_before, "config source changed"
                assert (ROOT / "dev/backend/local-s3.toml").read_bytes() == shared
                if (tools / "config-path").exists():
                    assert not Path((tools / "config-path").read_text()).exists(), (
                        "derived config remains"
                    )
                assert not list(run.glob("backend-ci.*")), "runtime data remains"
                for name in ("postgres", "s3", "backend"):
                    assert (run / "backend-ci-logs" / f"{name}.log").exists(), (
                        "service log missing"
                    )
                if mode == "bucket-failure":
                    assert (
                        "JSONDecodeError"
                        in (run / "backend-ci-logs/s3.log").read_text()
                    )
                for port in ports:
                    with socket.socket() as sock:
                        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                        sock.bind(("127.0.0.1", port))
                if (tools / "descendant").exists():
                    pid = int((tools / "descendant").read_text())
                    deadline = time.monotonic() + 1
                    while process_alive(pid) and time.monotonic() < deadline:
                        time.sleep(0.02)
                    assert not process_alive(pid), "test descendant remains alive"
                if (tools / "pids").exists():
                    for pid in (tools / "pids").read_text().splitlines():
                        assert not process_alive(int(pid)), (
                            f"owned process {pid} remains alive"
                        )
                print(f"PASS {mode} ({duration:.2f}s)", flush=True)
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=25)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                if sentinel is not None:
                    sentinel.close()
                # Contain deliberately broken cleanup mutations in the harness.
                if (tools / "pids").exists():
                    for pid in (tools / "pids").read_text().splitlines():
                        try:
                            os.kill(int(pid), signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                if (tools / "descendant").exists():
                    try:
                        os.kill(int((tools / "descendant").read_text()), signal.SIGKILL)
                    except ProcessLookupError:
                        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mutations",
        action="store_true",
        help="prove each check with a private temporary defect",
    )
    args = parser.parse_args()
    cases = (
        "success",
        "telemetry",
        "backend-wildcard",
        "metrics-wildcard",
        "listener-owner",
        "metrics-error",
        "exit23",
        "occupied",
        "bucket-failure",
        "unhealthy",
        "backend-exit",
        "setup-term",
        "child-term",
        "child-int",
    )
    for case in cases:
        check(case)
    if args.mutations:
        mutations = [
            ("success", ('rm -rf "$state"', ": # retain data")),
            (
                "exit23",
                (
                    'wait "${groups[${#groups[@]}-1]}"',
                    'wait "${groups[${#groups[@]}-1]}" || true',
                ),
            ),
            ("occupied", ("python3 - <<'PORTS'", "python3 - <<'PORTS' || true")),
            (
                "bucket-failure",
                (
                    "setup aws_local s3api put-bucket-policy",
                    "true || setup aws_local s3api put-bucket-policy",
                ),
            ),
            (
                "unhealthy",
                (
                    "until setup grpc-health-probe",
                    "until true || setup grpc-health-probe",
                ),
            ),
            ("unhealthy", ("sleep 5;", "sleep 120;")),
            ("backend-exit", ('kill -0 "$pid" 2>/dev/null ||', "true ||")),
            ("setup-term", ("trap 'exit 143' TERM", "trap 'groups=(); exit 143' TERM")),
            (
                "child-term",
                ('kill -TERM -- "-${groups[i]}"', 'kill -TERM "${groups[i]}"'),
            ),
            ("child-int", ("trap 'exit 130' INT", "trap 'groups=(); exit 130' INT")),
        ]
        # Corrupt the generated file after its runtime equality check. The
        # backend stub independently compares the actual file with its source.
        for old, new in (
            ("max_upload_bytes = 104857600", "max_upload_bytes = 1"),
            ("max_query_limit = 50", "max_query_limit = 51"),
            (
                'metrics_listen = "127.0.0.1:9464"',
                'metrics_listen = "127.0.0.1:9464"\nsample_ratio = 0.5',
            ),
            ('listen = "127.0.0.1:5050"', 'listen = "0.0.0.0:5050"'),
            ('metrics_listen = "127.0.0.1:9464"', 'metrics_listen = "0.0.0.0:9464"'),
        ):
            mutations.append(
                (
                    "success",
                    (
                        "out.write(text)",
                        f"out.write(text.replace({old!r}, {new!r}))",
                    ),
                )
            )
        mutations.extend(
            [
                ("success", ('"$state/config.toml"', '"$RUNNER_TEMP/config.toml"')),
                (
                    "success",
                    ("    out.write(text)", "    out.write(text)\ntarget.chmod(0o644)"),
                ),
                (
                    "backend-wildcard",
                    (
                        'setup python3 - "${services[2]}"',
                        'true || setup python3 - "${services[2]}"',
                    ),
                ),
                (
                    "metrics-wildcard",
                    (
                        'setup python3 - "${services[2]}"',
                        'true || setup python3 - "${services[2]}"',
                    ),
                ),
                (
                    "listener-owner",
                    (
                        'setup python3 - "${services[2]}"',
                        'true || setup python3 - "${services[2]}"',
                    ),
                ),
                (
                    "metrics-error",
                    (
                        'setup python3 - "${services[2]}"',
                        'true || setup python3 - "${services[2]}"',
                    ),
                ),
            ]
        )
        for case, mutation in mutations:
            if case == "child-term":
                # Keep the KILL fallback from hiding the same group defect.
                mutation = ('-- "-${groups[i]}"', '"${groups[i]}"')
            try:
                check(case, mutation)
            except (AssertionError, OSError) as error:
                print(f"DETECTED {case}: {mutation!r}: {error}", flush=True)
            else:
                raise AssertionError(f"mutation survived: {case}: {mutation[0]}")


if __name__ == "__main__":
    main()
