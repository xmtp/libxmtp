"""Check dev/clean-incremental on fake Cargo target directories."""

import fcntl
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "dev/clean-incremental"
DAY = 24 * 60 * 60


def age(path, seconds):
    """Set the mtime of `path` and everything under it `seconds` back."""
    when = time.time() - seconds
    for root, dirs, files in os.walk(path, topdown=False):
        for name in files + dirs:
            os.utime(os.path.join(root, name), (when, when))
    os.utime(path, (when, when))


class CleanIncrementalTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="xmtp clean incremental ")
        self.addCleanup(self.temp.cleanup)
        self.target = Path(self.temp.name) / "target"
        self.debug = self.profile("debug")

    def profile(self, name):
        profile = self.target / name
        profile.mkdir(parents=True)
        for lock in (".cargo-lock", ".cargo-build-lock"):
            (profile / lock).touch()
        return profile

    def crate(self, profile, name, days):
        """A rustc crate directory with one finalized session."""
        session = profile / "incremental" / name / "s-abc-def"
        session.mkdir(parents=True)
        (session / "dep-graph.bin").write_bytes(b"graph")
        (session.parent / "s-abc.lock").touch()
        age(session.parent, days * DAY)
        return session.parent

    def unit(self, profile, name, days):
        """A Kache adaptive unit with its own rustc crate directory."""
        unit = profile / "incremental.kache-auto/v1" / name
        (unit / "rustc/xmtp_mls-abc/s-abc-def").mkdir(parents=True)
        (unit / "state.json").write_text("{}")
        age(unit, days * DAY)
        return unit

    def run_script(self, *args, env=None):
        result = subprocess.run(
            [sys.executable, "-B", str(SCRIPT), *args],
            capture_output=True,
            text=True,
            env={**os.environ, "CI": "", **(env or {})},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def test_removes_only_state_unused_for_the_window(self):
        old_crate = self.crate(self.debug, "xmtp_db-old", days=4)
        new_crate = self.crate(self.debug, "xmtp_db-new", days=4)
        # rustc wrote a new session two levels down two hours ago.
        os.utime(new_crate / "s-abc-def", (time.time() - 7200,) * 2)
        old_unit = self.unit(self.debug, "aaaa", days=4)
        new_unit = self.unit(self.debug, "bbbb", days=2)
        # Kache compiled this unit an hour ago: only its rustc crate dir moved.
        used_unit = self.unit(self.debug, "cccc", days=4)
        os.utime(used_unit / "rustc/xmtp_mls-abc", (time.time() - 3600,) * 2)
        wasm = self.profile("wasm32-unknown-unknown/debug")
        wasm_crate = self.crate(wasm, "xmtp_mls-old", days=4)
        deps = self.debug / "deps/libxmtp_db.rlib"
        deps.parent.mkdir()
        deps.touch()
        age(deps.parent, 10 * DAY)

        out = self.run_script("--days", "3", str(self.target))

        self.assertFalse(old_crate.exists())
        self.assertFalse(old_unit.exists())
        self.assertFalse(wasm_crate.exists())
        self.assertTrue(new_crate.exists())
        self.assertTrue(new_unit.exists())
        self.assertTrue(used_unit.exists())
        self.assertTrue(deps.exists())
        self.assertEqual(list(self.debug.glob(".incremental-trash*")), [])
        self.assertIn("reclaimed", out)

    def test_minutes_window(self):
        crate = self.crate(self.debug, "xmtp_db-a", days=0)
        age(crate, 20 * 60)
        self.run_script("--minutes", "30", str(self.target))
        self.assertTrue(crate.exists())
        self.run_script("--minutes", "10", str(self.target))
        self.assertFalse(crate.exists())

    def test_skips_a_profile_while_cargo_holds_its_lock(self):
        crate = self.crate(self.debug, "xmtp_db-old", days=10)
        # Cargo holds `.cargo-build-lock` exclusively for the whole build.
        holder = subprocess.Popen(
            [
                sys.executable,
                "-c",
                "import fcntl, sys; f = open(sys.argv[1], 'rb');"
                " fcntl.flock(f, fcntl.LOCK_EX); print('locked', flush=True);"
                " sys.stdin.read()",
                str(self.debug / ".cargo-build-lock"),
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )
        self.addCleanup(holder.wait)
        self.addCleanup(holder.stdin.close)
        self.addCleanup(holder.stdout.close)
        self.assertEqual(holder.stdout.readline(), "locked\n")

        out = self.run_script(str(self.target))
        self.assertTrue(crate.exists())
        self.assertIn("a build is running", out)

        holder.stdin.close()
        holder.wait()
        self.run_script(str(self.target))
        self.assertFalse(crate.exists())

    def test_removes_trash_an_interrupted_run_left(self):
        leftover = self.debug / ".incremental-trash-1/0-xmtp_db-old"
        leftover.mkdir(parents=True)
        self.run_script(str(self.target))
        self.assertEqual(list(self.debug.glob(".incremental-trash*")), [])

    def test_auto_runs_once_a_day_and_never_in_ci(self):
        stamp = self.target / ".clean-incremental-stamp"
        first = self.crate(self.debug, "xmtp_db-first", days=4)
        self.assertEqual(
            self.run_script("--auto", str(self.target), env={"CI": "1"}), ""
        )
        self.assertTrue(first.exists())
        self.assertFalse(stamp.exists())

        self.assertEqual(self.run_script("--auto", str(self.target)), "")
        self.assertFalse(first.exists())
        self.assertTrue(stamp.exists())

        second = self.crate(self.debug, "xmtp_db-second", days=4)
        self.run_script("--auto", str(self.target))
        self.assertTrue(second.exists())

        age(stamp, DAY + 60)
        self.run_script("--auto", str(self.target))
        self.assertFalse(second.exists())

    def test_a_shared_cargo_lock_also_skips_the_profile(self):
        # Cargo 1.97 holds `.cargo-lock` shared during a build.
        with open(self.debug / ".cargo-lock", "rb") as held:
            fcntl.flock(held, fcntl.LOCK_SH)
            crate = self.crate(self.debug, "xmtp_db-old", days=10)
            self.run_script(str(self.target))
            self.assertTrue(crate.exists())


if __name__ == "__main__":
    unittest.main()
