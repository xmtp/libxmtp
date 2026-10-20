"""Check the runner classes that dev/ci/metrics reports."""

import importlib.machinery
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
LOADER = importlib.machinery.SourceFileLoader(
    "ci_metrics", str(ROOT / "dev/ci/metrics")
)
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
METRICS = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(METRICS)


class RunnerClassTests(unittest.TestCase):
    def test_runner_class_of_each_vendor(self):
        # Each paid vendor has its own class, so a run on Blacksmith macOS is
        # not counted as GitHub-hosted.
        cases = {
            "blacksmith-16vcpu-ubuntu-2404": ("Linux", "Blacksmith 16 vCPU"),
            "blacksmith-6vcpu-macos-15": ("macOS", "Blacksmith macOS 6 vCPU"),
            "blacksmith-12vcpu-macos-26": ("macOS", "Blacksmith macOS 12 vCPU"),
            "warp-macos-26-arm64-12x": ("macOS", "Warp macOS"),
            "macos-15": ("macOS", "GitHub macOS"),
            "ubuntu-24.04": ("Linux", "GitHub Linux"),
            "windows-latest": ("Windows", "GitHub Windows"),
        }
        for label, expected in cases.items():
            with self.subTest(label=label):
                self.assertEqual(METRICS.runner_class([label]), expected)


if __name__ == "__main__":
    unittest.main()
