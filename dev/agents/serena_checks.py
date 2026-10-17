"""Test broker identity and shutdown without starting a language server."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

import psutil

import serena_broker as broker


class BrokerTests(unittest.TestCase):
    def test_identity_includes_launcher(self):
        with tempfile.TemporaryDirectory(prefix="xmtp-serena-identity-") as temp:
            root = Path(temp)
            (root / "dev/agents").mkdir(parents=True)
            for name in (
                "serena_broker.py",
                "context.yml",
                "uv.lock",
                "pyproject.toml",
            ):
                (root / "dev/agents" / name).write_text(name)
            launcher = root / "dev/serena"
            launcher.write_text("original launcher")
            with patch.object(broker, "ROOT", root):
                initial = broker.identity()
                self.assertEqual(initial, broker.identity())
                launcher.write_text("changed launcher")
                self.assertNotEqual(initial, broker.identity())

    def test_graceful_shutdown_does_not_kill(self):
        process = Mock()
        with patch.object(broker, "wait_for_exit") as wait:
            broker.shutdown(process)
        wait.assert_called_once_with(process, 30)
        process.terminate.assert_called_once_with()
        process.kill.assert_not_called()

    def test_timeout_kills_server_and_children(self):
        process = Mock()
        child = Mock()
        process.children.return_value = [child]
        with (
            patch.object(
                broker, "wait_for_exit", side_effect=[psutil.TimeoutExpired(30), None]
            ) as wait,
            patch.object(broker.psutil, "wait_procs", return_value=([child], [])),
        ):
            broker.shutdown(process)
        process.children.assert_called_once_with(recursive=True)
        child.kill.assert_called_once_with()
        process.kill.assert_called_once_with()
        self.assertEqual(wait.call_args_list[-1].args, (process, 5))

    def test_shutdown_handles_already_exited_server(self):
        process = Mock()
        process.terminate.side_effect = psutil.NoSuchProcess(123)
        broker.shutdown(process)
        process.kill.assert_not_called()

    def test_shutdown_handles_child_that_exits_before_kill(self):
        process = Mock()
        child = Mock()
        child.kill.side_effect = psutil.NoSuchProcess(124)
        process.children.return_value = [child]
        with (
            patch.object(
                broker, "wait_for_exit", side_effect=[psutil.TimeoutExpired(30), None]
            ),
            patch.object(broker.psutil, "wait_procs", return_value=([child], [])),
        ):
            broker.shutdown(process)
        process.kill.assert_called_once_with()

    def test_wait_accepts_zombie_owned_by_another_client(self):
        process = Mock()
        process.wait.side_effect = psutil.TimeoutExpired(0.2)
        process.status.return_value = psutil.STATUS_ZOMBIE
        broker.wait_for_exit(process, 30)
        process.wait.assert_called_once()

    def test_wait_still_times_out_for_running_process(self):
        process = Mock()
        process.wait.side_effect = psutil.TimeoutExpired(0)
        process.status.return_value = psutil.STATUS_RUNNING
        with self.assertRaises(psutil.TimeoutExpired):
            broker.wait_for_exit(process, 0)

    def test_forced_shutdown_stops_real_process_tree(self):
        child_code = "import time; print('ready', flush=True); time.sleep(120)"
        server_code = (
            "import signal, subprocess, sys, time; "
            "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
            f"child = subprocess.Popen([sys.executable, '-c', {child_code!r}], stdout=subprocess.PIPE); "
            "child.stdout.readline(); print(child.pid, flush=True); time.sleep(120)"
        )
        owner = subprocess.Popen(
            [sys.executable, "-c", server_code],
            stdout=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        process = psutil.Process(owner.pid)
        child = None
        try:
            child = psutil.Process(int(owner.stdout.readline()))
            wait = broker.wait_for_exit
            with patch.object(
                broker,
                "wait_for_exit",
                side_effect=lambda target, timeout: wait(
                    target, 0.2 if timeout == 30 else timeout
                ),
            ):
                broker.shutdown(process)
            self.assertEqual(process.wait(timeout=5), -9)
            wait(child, 5)
        finally:
            for target in (child, process):
                if target is not None:
                    try:
                        target.kill()
                    except psutil.NoSuchProcess:
                        pass
            owner.wait(timeout=5)
            owner.stdout.close()


if __name__ == "__main__":
    unittest.main()
