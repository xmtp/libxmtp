"""Check the split's timing and test-inventory scripts without running a build."""

import importlib.machinery
import json
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def load(name: str):
    path = ROOT / "dev/split" / name
    loader = importlib.machinery.SourceFileLoader(name.replace("-", "_"), str(path))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


timing = load("timing")
inventory = load("test-inventory")


def cargo_says(stderr: str) -> list[str]:
    """A command that prints stderr as cargo would and succeeds."""
    return [sys.executable, "-c", f"import sys; sys.stderr.write({stderr!r})"]


class TimingRunTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="split timing ")
        self.addCleanup(self.temp.cleanup)
        self.file = Path(self.temp.name) / "edited.rs"
        self.file.write_text("fn main() {}\n")
        patcher = mock.patch.object(timing, "other_builds", return_value=[])
        patcher.start()
        self.addCleanup(patcher.stop)

    def test_a_run_that_compiles_the_package_is_kept_and_the_file_restored(self):
        run = timing.run_once(
            self.file,
            "xmtp_mls",
            "fn main() {}\n",
            cargo_says(
                "   Compiling xmtp_db v1.0.0 (/x)\n   Compiling xmtp_mls v1.0.0 (/y)\n"
            ),
        )
        self.assertEqual(run["compiled"], 2)
        self.assertEqual(self.file.read_text(), "fn main() {}\n")

    def test_a_run_that_does_not_compile_the_package_fails(self):
        with self.assertRaisesRegex(SystemExit, "did not compile xmtp_mls"):
            timing.run_once(
                self.file,
                "xmtp_mls",
                "fn main() {}\n",
                cargo_says("    Finished dev profile in 0.1s\n"),
            )
        self.assertEqual(self.file.read_text(), "fn main() {}\n")

    def test_cargo_colors_are_off_in_the_timed_command(self):
        command = [
            sys.executable,
            "-c",
            "import os, sys; color = os.environ.get('CARGO_TERM_COLOR');"
            " sys.stderr.write('   Compiling xmtp_mls v1 (/y)\\n' if color == 'never'"
            " else '\\x1b[1m\\x1b[32m   Compiling\\x1b[0m xmtp_mls v1 (/y)\\n')",
        ]
        with mock.patch.dict(os.environ, CARGO_TERM_COLOR="always"):
            run = timing.run_once(self.file, "xmtp_mls", "fn main() {}\n", command)
        self.assertEqual(run["compiled"], 1)

    def test_a_build_that_starts_during_the_run_stops_it(self):
        with (
            mock.patch.object(timing, "other_builds", side_effect=[[], ["42 rustc"]]),
            mock.patch.object(timing, "POLL_SECONDS", 0.01),
            self.assertRaisesRegex(SystemExit, "another build started during the run"),
        ):
            timing.run_once(
                self.file,
                "xmtp_mls",
                "fn main() {}\n",
                [sys.executable, "-c", "import time; time.sleep(5)"],
            )
        self.assertEqual(self.file.read_text(), "fn main() {}\n")

    def test_a_build_seen_only_after_the_run_stops_it(self):
        with (
            mock.patch.object(timing, "other_builds", side_effect=[[], ["42 rustc"]]),
            self.assertRaisesRegex(SystemExit, "another build ran during the run"),
        ):
            timing.run_once(
                self.file,
                "xmtp_mls",
                "fn main() {}\n",
                cargo_says("   Compiling xmtp_mls v1.0.0 (/y)\n"),
            )

    def test_the_recorded_time_ends_when_the_command_does(self):
        command = [
            sys.executable,
            "-c",
            "import sys, time; time.sleep(0.2); sys.stderr.write('   Compiling xmtp_mls v1 (/y)\\n')",
        ]
        with mock.patch.object(timing, "POLL_SECONDS", 5):
            run = timing.run_once(self.file, "xmtp_mls", "fn main() {}\n", command)
        self.assertLess(run["seconds"], 2)

    def test_another_build_stops_the_run_before_it_starts(self):
        with (
            mock.patch.object(timing, "other_builds", return_value=["42 rustc"]),
            self.assertRaisesRegex(SystemExit, "another build is running"),
        ):
            timing.run_once(self.file, "xmtp_mls", "fn main() {}\n", ["false"])
        self.assertEqual(self.file.read_text(), "fn main() {}\n")


class OtherBuildsTests(unittest.TestCase):
    def test_builds_outside_the_timed_tree_are_found(self):
        listing = (
            "  100     1 python3.11\n"
            "  101   100 cargo\n"
            "  102   101 /usr/local/bin/rustc\n"
            "  200     1 /Users/dev/.cargo/bin/cargo\n"
            "  300     1 bash\n"
        )
        with mock.patch.object(timing.subprocess, "run") as run:
            run.return_value.stdout = listing
            self.assertEqual(timing.other_builds(101), ["200 cargo"])
            self.assertEqual(
                timing.other_builds(), ["101 cargo", "102 rustc", "200 cargo"]
            )


class TimingEnvironmentTests(unittest.TestCase):
    def environment(self, kache=None, **settings):
        def run(command, **_):
            result = mock.Mock()
            result.stdout = kache if command[0] == "kache" else "fixture\n"
            return result

        with (
            mock.patch.dict(os.environ, settings),
            mock.patch.object(timing.subprocess, "run", side_effect=run),
            mock.patch.object(
                timing.shutil, "which", return_value=kache and "/bin/kache"
            ),
        ):
            return timing.environment()

    def test_credentials_are_redacted(self):
        found = self.environment(
            KACHE_S3_ACCESS_KEY="access-fixture",
            KACHE_S3_SECRET_KEY="secret-fixture",
            CARGO_REGISTRY_TOKEN="token-fixture",
            SCCACHE_REDIS="rediss://user:password-fixture@cache:6380/?db=12",
            KACHE_CACHE_DIR="/cache",
        )
        text = repr(found)
        for value in (
            "access-fixture",
            "secret-fixture",
            "token-fixture",
            "password-fixture",
        ):
            self.assertNotIn(value, text)
        self.assertEqual(found["settings"]["KACHE_S3_SECRET_KEY"], "<redacted>")
        self.assertEqual(found["settings"]["KACHE_CACHE_DIR"], "/cache")

    def test_build_settings_are_recorded(self):
        one = self.environment(CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="0")
        four = self.environment(CARGO_BUILD_JOBS="4", CARGO_INCREMENTAL="1")
        self.assertNotEqual(one["settings"], four["settings"])
        self.assertIn(
            "rustflags", repr(one["cargo_config"][str(ROOT / ".cargo/config.toml")])
        )

    def test_the_home_cargo_config_is_recorded_with_secrets_redacted(self):
        with tempfile.TemporaryDirectory() as home:
            config = Path(home) / "config.toml"
            config.write_text(
                '[build]\njobs = 3\n[registry]\n"token" = """first-fixture\nsecond-fixture"""\n'
                '[registries.private]\nindex = "https://user:url-fixture@example.com/index"\n'
                '[env]\nCARGO_REGISTRIES_PRIVATE_TOKEN = { value = "inline-fixture", force = true }\n'
            )
            three = self.environment(CARGO_HOME=home)
            config.write_text("[build]\njobs = 5\n")
            five = self.environment(CARGO_HOME=home)
        self.assertEqual(three["cargo_config"][str(config)]["build"]["jobs"], 3)
        for value in (
            "first-fixture",
            "second-fixture",
            "url-fixture",
            "inline-fixture",
        ):
            self.assertNotIn(value, repr(three))
        self.assertNotEqual(three["cargo_config"], five["cargo_config"])

    def test_the_kache_config_files_are_recorded_with_secrets_redacted(self):
        with tempfile.TemporaryDirectory() as home:
            config = Path(home) / "with space" / "kache.toml"
            config.parent.mkdir()
            host = Path(home) / "host.toml"
            host.write_text("cache_executables = false\n")
            config.write_text(
                '[remote]\nsecret_key = "secret-fixture"\nbucket = "cache"\n'
            )
            doctor = json.dumps(
                {
                    "checks": [
                        {"label": "Config file", "detail": f"{config} (exists)"},
                        {"label": "Cache dir", "detail": "/cache"},
                        {
                            "label": "Host config",
                            "detail": f"{host}: cache_executables " + "x" * 300,
                        },
                    ]
                }
            )
            found = self.environment(kache=doctor)["kache_doctor"]
        self.assertEqual(found["config"][str(config)]["remote"]["bucket"], "cache")
        self.assertEqual(found["config"][str(host)], {"cache_executables": False})
        self.assertNotIn("secret-fixture", repr(found))
        self.assertEqual(found["checks"]["Cache dir"], "/cache")


class InventoryTests(unittest.TestCase):
    def test_leaves_are_sorted_with_duplicates_kept(self):
        listing = {
            "rust-suites": {
                "b": {"testcases": {"m::z_test": {}, "m::a_test": {}}},
                "a": {"testcases": {"other::a_test": {}}},
            }
        }
        self.assertEqual(inventory.leaves(listing), ["a_test", "a_test", "z_test"])


if __name__ == "__main__":
    unittest.main()
