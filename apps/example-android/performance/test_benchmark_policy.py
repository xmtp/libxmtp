from pathlib import Path
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[3]


class BenchmarkPolicyTest(unittest.TestCase):
    def test_workload_has_only_an_explicit_workflow_owner(self):
        normal = yaml.safe_load(
            (ROOT / ".github/workflows/test-android.yml").read_text()
        )
        manual = yaml.safe_load(
            (ROOT / ".github/workflows/benchmark-example-android.yml").read_text()
        )
        self.assertEqual(
            {"workflow_dispatch"}, set(manual["on"]), "Benchmark has an automatic event"
        )
        self.assertNotIn(
            "messenger-performance",
            normal["jobs"],
            "Benchmark runs in normal Android CI",
        )
        expected = {
            "unit-tests",
            "min-sdk-smoke",
            "integration-tests",
            "check-android",
            "android-stage",
        }
        self.assertEqual(expected, set(normal["jobs"]["results"]["needs"]))
        passed = normal["jobs"]["results"]["steps"][0]["env"]["PASSED"]
        self.assertNotIn("needs.messenger-performance", passed)
        for job in expected:
            self.assertIn("needs." + job + ".result == 'success'", passed)
        benchmark = manual["jobs"]["messenger-performance"]
        self.assertEqual("blacksmith-16vcpu-ubuntu-2404", benchmark["runs-on"])
        commands = [step.get("run", "") for step in benchmark["steps"]]
        self.assertIn("dev/nix-shell 'just example-android performance'", commands)
        self.assertFalse(
            any(
                "example-performance" in step.get("run", "")
                for job in normal["jobs"].values()
                for step in job.get("steps", [])
            )
        )


if __name__ == "__main__":
    unittest.main()
