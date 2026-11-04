"""Exercise snapshot setup without installing Nix or changing the host."""

import os
import shutil
import shlex
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class DroidSetupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="droid setup ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "home"
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.home.mkdir()
        (self.root / ".factory").mkdir()
        (self.root / "dev").mkdir()
        self.global_bin = self.root / "global/bin"
        self.config = self.root / "etc/nix"
        self.config.mkdir(parents=True)
        (self.config / "nix.conf").write_text("# existing configuration\n")
        (self.root / ".factory/nix-daemon.sh").write_text(
            '#!/bin/sh\nprintf "daemon ready\\n" >> "$SETUP_LOG"\n'
            'exit "${FAIL_DAEMON:-0}"\n'
        )
        self.script = self.root / ".factory/setup.sh"
        # Never inspect or use the test machine's global Nix installation.
        self.script.write_text(
            (ROOT / ".factory/setup.sh")
            .read_text()
            .replace("/nix/var/nix/profiles/default", str(self.root / "global"))
            .replace("/etc/nix", str(self.config))
            .replace("/usr/local/sbin/libxmtp-nix-daemon", str(self.root / "helper"))
        )
        self.log = self.root / "calls"
        self.env = {
            **os.environ,
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.home / ".config"),
            "PATH": f"{self.bin}:/usr/bin:/bin",
            "SETUP_LOG": str(self.log),
        }
        self.env.pop("NIX_CONFIG", None)
        self.write_tool(self.bin / "uname", "echo Linux\n")
        self.write_tool(self.bin / "sudo", '[ "$1" != -n ] || shift\nexec "$@"\n')
        self.write_tool(self.bin / "install", 'shift 4\nexec /usr/bin/install "$@"\n')
        self.write_tool(
            self.root / "dev/nix-shell",
            'printf "shell %s\\n" "$*" >> "$SETUP_LOG"\n'
            'if [ "${FAIL_CHECK:-}" = 1 ] && [ "$1" = "just check" ]; then exit 43; fi\n'
            'if [ -n "${FAIL_SHELL:-}" ] && [ "$FAIL_SHELL" = "${2:-}" ]; then exit 42; fi\n',
        )
        self.installer = self.root / "installer"
        self.installer.write_text(
            'printf "install %s\\n" "$*" >> "$SETUP_LOG"\n'
            'mkdir -p "$GLOBAL_BIN"\n'
            'cp "$FAKE_NIX" "$GLOBAL_BIN/nix"\n'
            'cp "$FAKE_NIX" "$GLOBAL_BIN/nix-daemon"\n'
        )
        self.fake_nix = self.root / "fake-nix"
        self.write_tool(self.fake_nix, 'echo "nix $*" >> "$SETUP_LOG"\n')
        self.env["FAKE_NIX"] = str(self.fake_nix)
        self.env["INSTALLER"] = str(self.installer)
        self.env["GLOBAL_BIN"] = str(self.global_bin)
        self.write_tool(
            self.bin / "curl",
            'while [ "$1" != --output ]; do shift; done\ncp "$INSTALLER" "$2"\n',
        )
        # Dependency discovery must never invoke the host package manager.
        for name in ("xz", "git", "shasum", "flock", "start-stop-daemon"):
            self.write_tool(self.bin / name, "exit 0\n")
        self.write_tool(self.bin / "apt-get", "exit 98\n")

    def write_tool(self, path, body):
        path.write_text("#!/bin/sh\nset -eu\n" + body)
        path.chmod(0o755)

    def run_setup(self):
        return subprocess.run(
            ["/bin/bash", str(self.script)],
            env=self.env,
            cwd=self.root.parent,
            capture_output=True,
            text=True,
        )

    def test_install_warms_shells_and_rerun_preserves_configuration(self):
        config = self.config
        (self.home / ".bashrc").write_text("# existing shell configuration\n")
        for _ in range(2):
            result = self.run_setup()
            self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.log.read_text()
        self.assertEqual(
            calls.count("install install linux --no-confirm --init none"), 1
        )
        self.assertEqual(calls.count("daemon ready\n"), 2)
        for shell in ("default", "rust", "js"):
            self.assertEqual(calls.count(f"shell --shell {shell} true\n"), 2)
        self.assertEqual(calls.count("shell just check\n"), 2)
        conf = (config / "nix.conf").read_text()
        self.assertIn("# existing configuration", conf)
        self.assertEqual(conf.count("include "), 1)
        cache = (config / "libxmtp-droid.conf").read_text()
        self.assertIn("extra-substituters = https://xmtp.cachix.org", cache)
        self.assertIn("xmtp.cachix.org-1:nFPFrq", cache)
        self.assertIn("sandbox = true\n", cache)
        self.assertIn("sandbox-fallback = false\n", cache)
        self.assertNotIn("build-users-group =", cache)
        profile = (self.home / ".bashrc").read_text()
        self.assertEqual(profile.count("# libxmtp Droid Nix"), 1)
        self.assertIn("# existing shell configuration", profile)

    def test_failed_shell_prevents_snapshot_success(self):
        self.env["FAIL_SHELL"] = "rust"
        result = self.run_setup()
        self.assertEqual(result.returncode, 42, result.stderr)
        self.assertNotIn("shell --shell js", self.log.read_text())
        self.assertNotIn("ready for snapshotting", result.stdout)

    def test_failed_daemon_start_prevents_shell_and_cargo_warmup(self):
        self.env["FAIL_DAEMON"] = "44"
        result = self.run_setup()
        self.assertEqual(result.returncode, 44, result.stderr)
        self.assertNotIn("shell ", self.log.read_text())

    def test_existing_single_user_install_requires_fresh_source(self):
        shutil.copy2(self.fake_nix, self.bin / "nix")
        result = self.run_setup()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("single-user Nix is unsupported", result.stderr)
        self.assertFalse(self.log.exists())

    def test_failed_cargo_check_prevents_snapshot_success(self):
        self.env["FAIL_CHECK"] = "1"
        result = self.run_setup()
        self.assertEqual(result.returncode, 43, result.stderr)
        self.assertIn("shell just check\n", self.log.read_text())
        self.assertNotIn("ready for snapshotting", result.stdout)


class NixDaemonTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="droid daemon ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        (self.root / "run").mkdir()
        self.script = self.root / "daemon.sh"
        self.script.write_text(
            (ROOT / ".factory/nix-daemon.sh")
            .read_text()
            .replace(
                "export PATH=/usr/sbin:/usr/bin:/sbin:/bin",
                f'export PATH="{self.bin}:/usr/bin:/bin"',
            )
            .replace("/nix/var/nix/profiles/default/bin", str(self.bin))
            .replace("/run/libxmtp", str(self.root / "run/libxmtp"))
            .replace("/usr/sbin/start-stop-daemon", str(self.bin / "start-stop-daemon"))
        )
        self.ready = self.root / "ready"
        self.log = self.root / "daemon-env"
        self.write_tool("id", "echo 0\n")
        self.write_tool("flock", "exit 0\n")
        self.write_tool("sleep", "exit 0\n")
        self.write_tool("nix", f"[ -f {shlex.quote(str(self.ready))} ]\n")
        self.write_tool(
            "start-stop-daemon",
            'while [ "$1" != --exec ]; do shift; done\nexec "$2"\n',
        )
        self.write_tool(
            "nix-daemon",
            f"/usr/bin/env > {shlex.quote(str(self.log))}\n"
            f": > {shlex.quote(str(self.ready))}\n",
        )
        self.env = {
            "PATH": f"{self.bin}:/usr/bin:/bin",
            "SESSION_VALUE": "fixture",
        }
        self.env["NIX_CONFIG"] = "sandbox = false"

    def write_tool(self, name, body):
        path = self.bin / name
        path.write_text("#!/bin/sh\nset -eu\n" + body)
        path.chmod(0o755)

    def run_daemon(self):
        return subprocess.run(
            ["/bin/bash", str(self.script)],
            env=self.env,
            capture_output=True,
            text=True,
        )

    def test_starts_with_clean_environment_and_reuses_running_daemon(self):
        result = self.run_daemon()
        self.assertEqual(result.returncode, 0, result.stderr)
        environment = self.log.read_text()
        self.assertIn("sandbox = true\nsandbox-fallback = false", environment)
        self.assertIn("NIX_USER_CONF_FILES=/dev/null", environment)
        self.assertNotIn("SESSION_VALUE=", environment)
        self.assertNotIn("sandbox = false", environment)
        self.log.unlink()
        result = self.run_daemon()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.log.exists(), "A healthy daemon must not restart")

    def test_unready_daemon_fails_instead_of_using_local_store(self):
        self.write_tool("nix-daemon", "exit 0\n")
        result = self.run_daemon()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("did not become ready", result.stderr)

    def test_non_root_cannot_start_daemon(self):
        self.write_tool("id", "echo 1000\n")
        result = self.run_daemon()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("requires root", result.stderr)
        self.assertFalse(self.log.exists())


if __name__ == "__main__":
    unittest.main()
