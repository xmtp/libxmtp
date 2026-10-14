#!/usr/bin/env python3
"""Check the process contract of the test services (nix/services, dev/services).

The checks run the storage services (PostgreSQL, the replica, and S3) of
`.#storageServices`, so they need no backend build. Every port is a free
loopback port, so a running worktree stack does not interfere.
"""

import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import tempfile
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
PORTS = ("XMTP_BACKEND_DB_PORT", "XMTP_BACKEND_REPLICA_PORT", "XMTP_S3_PORT")
URLS = ("DATABASE_URL", "XMTP_REPLICA_URL", "XMTP_S3_URL", "XMTP_S3_BASE_URL")


def free_ports(count):
    sockets = [socket.socket() for _ in range(count)]
    for sock in sockets:
        sock.bind(("127.0.0.1", 0))
    ports = [sock.getsockname()[1] for sock in sockets]
    for sock in sockets:
        sock.close()
    return ports


def listening(port):
    try:
        socket.create_connection(("127.0.0.1", port), 0.5).close()
        return True
    except OSError:
        return False


def read_env(path):
    values = {}
    for line in path.read_text().splitlines():
        name, value = line.removeprefix("export ").split("=", 1)
        values[name] = value.strip("'")
    return values


def stop_group(process):
    """Stop a process started with start_new_session and all of its group.

    services-test leaves process-compose and the services in its group, so a
    timeout must stop them too, not only the wrapper.
    """

    def send(sig):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            pass
        except PermissionError:
            # Darwin returns EPERM while the group leader is exiting.
            process.wait(timeout=10)

    send(signal.SIGTERM)
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass
    send(signal.SIGKILL)
    process.wait(timeout=10)


def run_group(args, *, timeout, check=False, **kwargs):
    """subprocess.run in a new session; a timeout stops the whole group."""
    with subprocess.Popen(
        args,
        start_new_session=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        **kwargs,
    ) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            stop_group(process)
            raise
    result = subprocess.CompletedProcess(args, process.returncode, stdout, stderr)
    if check:
        result.check_returncode()
    return result


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


class ServicesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        out = subprocess.run(
            [
                "nix",
                "build",
                "--no-link",
                "--print-out-paths",
                f"{ROOT}#storageServices.test",
                f"{ROOT}#storageServices.package",
            ],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.split()
        cls.test_bin = Path(out[0]) / "bin/services-test"
        cls.package = Path(out[1])

    def setUp(self):
        # A short directory: macOS limits a socket path to 104 bytes.
        temporary = tempfile.TemporaryDirectory(prefix="xs-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # dev/nix-shell loads this worktree's ports; the services must not
        # see them, or a running worktree stack would hold the ports.
        self.env = {
            name: value
            for name, value in os.environ.items()
            if not name.startswith(("XMTP_", "PC_"))
            and name not in ("DATABASE_URL", "ANVIL_URL")
        }

    def services_test(self, command, **env):
        return run_group(
            [str(self.test_bin)],
            cwd=self.root,
            env={**self.env, "XMTP_SERVICES_TEST_COMMAND": command, **env},
            timeout=180,
        )

    def test_command_sees_the_env_file_and_sets_the_exit_status(self):
        result = self.services_test(
            'cp "$XMTP_SERVICES_STATE/env" exported; env > seen; exit 3'
        )
        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        exported = read_env(self.root / "exported")
        self.assertEqual(set(exported), {*PORTS, *URLS})
        ports = {int(exported[name]) for name in PORTS}
        self.assertEqual(len(ports), len(PORTS), exported)
        port = exported["XMTP_BACKEND_DB_PORT"]
        self.assertEqual(
            exported["DATABASE_URL"],
            f"postgres://xmtp:xmtp@127.0.0.1:{port}/xmtp_backend",
        )
        seen = dict(
            line.split("=", 1)
            for line in (self.root / "seen").read_text().splitlines()
            if "=" in line
        )
        for name, value in exported.items():
            self.assertEqual(seen.get(name), value, name)
        # The wrapper removes the state directory that it created.
        self.assertFalse(Path(seen["XMTP_SERVICES_STATE"]).exists())
        for port in ports:
            self.assertFalse(listening(port), f"port {port} still listens")

    def test_failed_command_prints_the_service_logs(self):
        # A Nix build deletes its TMPDIR, so the logs must reach the output.
        result = self.services_test(
            "echo FAIL some::test; exit 3",
            XMTP_SERVICES_LOGS=str(self.root / "logs"),
        )
        self.assertEqual(result.returncode, 3, result.stderr)
        for name in ("db", "replica", "s3"):
            self.assertIn(
                f"--- last lines of {self.root}/logs/{name}.log", result.stderr
            )
        # Nix shows only the end of a failed build: the command output and
        # the exit status come after the service logs.
        self.assertEqual(
            result.stderr.splitlines()[-3:],
            [
                "--- last lines of the test command output",
                "FAIL some::test",
                "services-test: exit status 3",
            ],
        )

    def test_term_stops_every_service(self):
        # A killed `just backend ci` must not leave services on the ports.
        ports = free_ports(len(PORTS))
        state = self.root / "state"
        run = subprocess.Popen(
            [str(self.test_bin)],
            cwd=self.root,
            env={
                **self.env,
                **dict(zip(PORTS, map(str, ports))),
                "XMTP_SERVICES_STATE": str(state),
                "XMTP_SERVICES_TEST_COMMAND": "sleep 300",
            },
            start_new_session=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.addCleanup(stop_group, run)
        deadline = time.monotonic() + 120
        while not (state / "test-started").exists() and time.monotonic() < deadline:
            time.sleep(0.2)
        self.assertTrue((state / "test-started").exists(), "the command never started")
        run.send_signal(signal.SIGTERM)
        run.wait(timeout=60)
        deadline = time.monotonic() + 10
        while any(map(listening, ports)) and time.monotonic() < deadline:
            time.sleep(0.2)
        self.assertEqual([port for port in ports if listening(port)], [])

    def test_caller_port_in_use_fails_before_any_service_starts(self):
        with socket.socket() as holder:
            holder.bind(("127.0.0.1", 0))
            holder.listen()
            port = holder.getsockname()[1]
            result = self.services_test("touch ran", XMTP_S3_PORT=str(port))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(f"XMTP_S3_PORT={port} is in use", result.stderr)
        self.assertFalse((self.root / "ran").exists())

    def test_busy_caller_port_waits_when_asked(self):
        holder = socket.socket()
        holder.bind(("127.0.0.1", 0))
        holder.listen()
        port = holder.getsockname()[1]
        try:
            run = subprocess.Popen(
                [str(self.test_bin)],
                cwd=self.root,
                env={
                    **self.env,
                    "XMTP_SERVICES_TEST_COMMAND": "touch ran",
                    "XMTP_S3_PORT": str(port),
                    "XMTP_SERVICES_PORT_WAIT": "60",
                },
                start_new_session=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                text=True,
            )
            # Release the port only after the start reports that it waits, so
            # a slow start cannot miss the busy port.
            waited = threading.Event()
            lines = []

            def read_stderr():
                for line in run.stderr:
                    lines.append(line)
                    if f"XMTP_S3_PORT={port} is in use; waiting" in line:
                        waited.set()

            reader = threading.Thread(target=read_stderr, daemon=True)
            reader.start()
            waited.wait(timeout=30)
        finally:
            holder.close()
        with run.stderr:
            try:
                run.wait(timeout=180)
            except subprocess.TimeoutExpired:
                stop_group(run)
                raise
            reader.join(timeout=10)
        stderr = "".join(lines)
        self.assertTrue(waited.is_set(), stderr)
        self.assertEqual(run.returncode, 0, stderr)
        self.assertTrue((self.root / "ran").exists())

    def test_duplicate_caller_ports_fail_before_any_service_starts(self):
        (port,) = free_ports(1)
        result = self.services_test(
            "touch ran", XMTP_BACKEND_DB_PORT=str(port), XMTP_S3_PORT=str(port)
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(f"XMTP_S3_PORT={port} duplicates", result.stderr)
        self.assertFalse((self.root / "ran").exists())

    def test_skipped_dependency_fails_without_running_the_command(self):
        state = self.root / "state"
        # initdb refuses a data directory that is not empty, so db-init fails
        # and every process that depends on it is skipped.
        (state / "data/primary").mkdir(parents=True)
        (state / "data/primary/occupied").touch()
        result = self.services_test(
            "touch ran",
            XMTP_SERVICES_STATE=str(state),
            XMTP_SERVICES_LOGS=str(self.root / "logs"),
        )
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse((self.root / "ran").exists())
        # A startup failure prints the end of each service log.
        self.assertIn("db-init.log", result.stderr)
        self.assertIn("exists but is not empty", result.stderr)
        # S3 started; it must be stopped.
        s3 = int(read_env(state / "env")["XMTP_S3_PORT"])
        self.assertFalse(listening(s3), "S3 still listens")

    def dev_services(self, state):
        """Return a runner of `dev/services` in a fixture checkout."""
        # dev/worktree-env writes the fixture's own dev/docker/.env.
        repo = self.root / "repo"
        for path in ("dev/services", "dev/worktree-env", "dev/docker/load-env"):
            (repo / path).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / path, repo / path)
        subprocess.run(["git", "init", "-q", str(repo)], check=True)
        env = {
            **self.env,
            **dict(zip(PORTS, map(str, free_ports(len(PORTS))))),
            "XMTP_SERVICES_PACKAGE": str(self.package),
            "PC_SOCKET_PATH": str(self.root / "pc.sock"),
        }
        if state is not None:
            env["XMTP_SERVICES_STATE"] = str(state)

        def services(*args, check=True):
            return run_group(
                [str(repo / "dev/services"), *args],
                cwd=repo,
                env=env,
                timeout=200,
                check=check,
            )

        return services, env

    def test_dev_services_finds_its_state_from_another_shell(self):
        # dev/nix-shell gives each call its own TMPDIR; `env` and `down` run
        # in another call than `up`.
        services, env = self.dev_services(None)
        self.addCleanup(services, "down", check=False)
        first, second = self.root / "tmp-up", self.root / "tmp-later"
        first.mkdir()
        second.mkdir()
        env["TMPDIR"] = str(first)
        services("up")
        env["TMPDIR"] = str(second)
        self.assertIn("export DATABASE_URL=", services("env").stdout)
        services("down")
        self.assertEqual(list(self.root.glob("pc.st*")), [])

    def test_dev_services_down_keeps_an_unmarked_directory(self):
        # The caller sets the state path; a mistake must not delete a directory.
        state = self.root / "mine"
        state.mkdir()
        (state / "keep").touch()
        services, _ = self.dev_services(state)
        result = services("down", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not removing it", result.stderr)
        self.assertTrue((state / "keep").exists())

    def test_dev_services_down_leaves_nothing(self):
        state = self.root / "state"
        socket_path = self.root / "pc.sock"
        services, env = self.dev_services(state)
        self.addCleanup(services, "down", check=False)
        up = services("up", check=False)
        self.assertEqual(up.returncode, 0, up.stdout + up.stderr)
        self.assertEqual(read_env(state / "env")["XMTP_S3_PORT"], env["XMTP_S3_PORT"])
        self.assertIn("export DATABASE_URL=", services("env").stdout)
        listed = subprocess.run(
            [str(self.package / "bin/services"), "process", "list", "-o", "json"],
            env=env,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
        running = {
            process["name"]: process["pid"]
            for process in json.loads(listed)
            if process["status"] == "Running"
        }
        self.assertEqual(set(running), {"db", "replica", "s3"}, listed)
        self.assertIn("replica", services("ps").stdout)

        services("down")
        deadline = time.monotonic() + 5
        while any(map(alive, running.values())) and time.monotonic() < deadline:
            time.sleep(0.1)
        left = [name for name, pid in running.items() if alive(pid)]
        self.assertEqual(left, [], "service processes remain")
        self.assertFalse(state.exists(), "state directory remains")
        self.assertFalse(socket_path.exists(), "socket remains")


class WasmLockTests(unittest.TestCase):
    def test_wasm_partitions_lock_by_descriptor(self):
        # Each build can run as another build user. Linux
        # fs.protected_regular refuses an O_CREAT open of another user's file
        # in /tmp, and `flock <path>` always passes O_CREAT, so every flock
        # call must name a descriptor.
        source = (ROOT / "nix/package/wasm-nextest.nix").read_text()
        calls = [
            match.group(1).split()
            for line in source.splitlines()
            for match in re.finditer(
                r"(?:^|[;&|!]|\bthen\b|\bdo\b)\s*flock\s+([^;&|]*)",
                line.split("#", 1)[0].strip(),
            )
        ]
        self.assertTrue(calls, "wasm-nextest.nix no longer calls flock")
        for args in calls:
            operands = [arg for arg in args if not arg.startswith("-")]
            self.assertTrue(
                operands and all(arg.isdigit() for arg in operands),
                f"flock {' '.join(args)}: lock a descriptor, not a path",
            )


if __name__ == "__main__":
    unittest.main()
