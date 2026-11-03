"""Exercise the readiness guards with disposable tracked files and dependencies."""

import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "dev" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


guidance = load("check_agent_guidance")
large = load("check_large_files")


class ReadinessChecksTests(unittest.TestCase):
    def test_guidance_references_and_commands(self):
        tree = json.loads(
            subprocess.check_output(
                ["just", "--dump", "--dump-format", "json"], cwd=ROOT, text=True
            )
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "dev").mkdir()
            (root / "dev/real.md").touch()
            (root / "nested").mkdir()
            (root / "nested/src").mkdir()
            doc = root / "nested/AGENTS.md"
            for text, expected in (
                ("[local](../dev/real.md#heading)", None),
                ("[root](dev/real.md)", None),
                ("[external](https://example.org/missing)", None),
                ("[anchor](#heading)", None),
                ("`dev/real.md`", None),
                ("`dev/*.md` `target/output.json` `/pure` `./gradlew`", None),
                ("`just backend up` `just backend::up` `just test crate foo`", None),
                ("`just check workspace` `just --list` `just <recipe>`", None),
                ("[broken](../dev/missing.md)", "missing link target"),
                ("`dev/missing.md`", "missing repository path"),
                ("`../dev/missing.md`", "missing repository path"),
                ("`src/missing.py`", "missing repository path"),
                ("`just missing-recipe`", "unknown Just recipe"),
                ("`just backend missing-recipe`", "unknown Just recipe"),
                ("`just backend::missing-recipe`", "unknown Just recipe"),
                ("`just test missing-target`", "unknown Just target"),
            ):
                with self.subTest(text=text):
                    doc.write_text(text)
                    failures = guidance.check_document(root, doc, tree)
                    if expected is None:
                        self.assertEqual(failures, [])
                    else:
                        self.assertEqual(len(failures), 1, failures)
                        self.assertIn(f"nested/AGENTS.md:1: {expected}", failures[0])

    def test_tracked_file_size_boundaries_and_exceptions(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            subprocess.run(["git", "init", "-q", folder], check=True)
            file = root / "with space.bin"
            file.write_bytes(b"x" * large.LIMIT)
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            self.assertEqual(large.check(root), [])
            file.write_bytes(b"x" * (large.LIMIT + 1))
            self.assertEqual(len(large.check(root)), 1)
            self.assertIn("with space.bin:", large.check(root)[0])
            self.assertEqual(large.check(root, {"with space.bin"}), [])
            file.unlink()
            self.assertEqual(large.check(root), [])
            (root / "untracked.bin").write_bytes(b"x" * (large.LIMIT + 1))
            self.assertEqual(large.check(root), [])

    def test_dependency_guard_missing_and_installed(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "dev/js").mkdir(parents=True)
            guard = root / "dev/js/require-deps.sh"
            shutil.copyfile(ROOT / "dev/js/require-deps.sh", guard)
            for package in ("apps/cli", "apps/web-chat", "apps/docs"):
                with self.subTest(package=package):
                    command = ["bash", str(guard), package]
                    missing = subprocess.run(command, capture_output=True, text=True)
                    self.assertEqual(missing.returncode, 1)
                    self.assertIn("dev/nix-shell 'just install-js'", missing.stderr)
                    (root / package / "node_modules").mkdir(parents=True)
                    installed = subprocess.run(command, capture_output=True, text=True)
                    self.assertEqual(installed.returncode, 0, installed.stderr)


if __name__ == "__main__":
    unittest.main()
