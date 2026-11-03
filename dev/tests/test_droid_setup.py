"""Exercise snapshot setup without installing Nix or changing the host."""

import os
import shutil
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
        self.script = self.root / ".factory/setup.sh"
        # Never inspect or use the test machine's global Nix installation.
        self.script.write_text(
            (ROOT / ".factory/setup.sh")
            .read_text()
            .replace("/nix/var/nix/profiles/default", str(self.root / "global"))
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
        self.write_tool(self.bin / "nix", "echo 'unexpected host nix' >&2; exit 99\n")
        # command -v must not find a host Nix before installation.
        (self.bin / "nix").unlink()
        self.write_tool(
            self.root / "dev/nix-shell",
            'printf "shell %s\\n" "$*" >> "$SETUP_LOG"\n'
            'if [ -n "${FAIL_SHELL:-}" ] && [ "$FAIL_SHELL" = "${2:-}" ]; then exit 42; fi\n',
        )
        self.installer = self.root / "installer"
        self.installer.write_text(
            'printf "install %s\\n" "$*" >> "$SETUP_LOG"\n'
            'printf "config %s\\n" "$NIX_CONFIG" >> "$SETUP_LOG"\n'
            'mkdir -p "$HOME/.nix-profile/bin"\n'
            'cp "$FAKE_NIX" "$HOME/.nix-profile/bin/nix"\n'
        )
        self.fake_nix = self.root / "fake-nix"
        self.write_tool(self.fake_nix, 'echo "nix $*" >> "$SETUP_LOG"\n')
        self.env["FAKE_NIX"] = str(self.fake_nix)
        self.env["INSTALLER"] = str(self.installer)
        self.write_tool(
            self.bin / "curl",
            'while [ "$1" != --output ]; do shift; done\ncp "$INSTALLER" "$2"\n',
        )
        # Dependency discovery must never invoke the host package manager.
        for name in ("xz", "git", "shasum"):
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
        config = self.home / ".config/nix"
        config.mkdir(parents=True)
        (config / "nix.conf").write_text("# existing configuration\n")
        (self.home / ".bashrc").write_text("# existing shell configuration\n")
        for _ in range(2):
            result = self.run_setup()
            self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.log.read_text()
        self.assertEqual(calls.count("install --no-daemon --yes"), 1)
        self.assertIn("--no-channel-add --no-modify-profile", calls)
        self.assertIn("build-users-group =", calls)
        for shell in ("default", "rust", "js"):
            self.assertEqual(calls.count(f"shell --shell {shell} true\n"), 2)
        conf = (config / "nix.conf").read_text()
        self.assertIn("# existing configuration", conf)
        self.assertEqual(conf.count("include "), 1)
        cache = (config / "libxmtp-droid.conf").read_text()
        self.assertIn("extra-substituters = https://xmtp.cachix.org", cache)
        self.assertIn("xmtp.cachix.org-1:nFPFrq", cache)
        profile = (self.home / ".bashrc").read_text()
        self.assertEqual(profile.count("# libxmtp Droid Nix"), 1)
        self.assertIn("# existing shell configuration", profile)

    def test_failed_shell_prevents_snapshot_success(self):
        binary = self.home / ".nix-profile/bin"
        binary.mkdir(parents=True)
        shutil.copy2(self.fake_nix, binary / "nix")
        self.env["FAIL_SHELL"] = "rust"
        result = self.run_setup()
        self.assertEqual(result.returncode, 42, result.stderr)
        self.assertNotIn("install ", self.log.read_text())
        self.assertNotIn("shell --shell js", self.log.read_text())
        self.assertNotIn("ready for snapshotting", result.stdout)


if __name__ == "__main__":
    unittest.main()
