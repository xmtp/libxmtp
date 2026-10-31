"""The materialization action preserves arguments on each runner's Bash."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]


class ReleaseSourceTest(unittest.TestCase):
    def test_materialization_arguments_with_and_without_a_snapshot(self):
        action = yaml.safe_load(
            (ROOT / ".github/actions/release-source/action.yml").read_text()
        )
        script = next(
            step["run"]
            for step in action["runs"]["steps"]
            if step.get("name") == "Materialize Cargo and all SDK metadata"
        )
        for runner in ("Linux", "macOS", "Windows"):
            for snapshot in (False, True):
                with self.subTest(runner=runner, snapshot=snapshot):
                    with tempfile.TemporaryDirectory(
                        prefix="release source "
                    ) as folder:
                        root = Path(folder)
                        capture = root / "arguments"
                        for name in ("dev/nix-shell", "bin/python"):
                            command = root / name
                            command.parent.mkdir(parents=True, exist_ok=True)
                            command.write_text(
                                '#!/bin/sh\nprintf "%s\\n" "$@" > "$CAPTURE_FILE"\n'
                            )
                            command.chmod(0o755)
                        runner_temp = root / "runner temp"
                        result = subprocess.run(
                            [
                                "bash",
                                "--noprofile",
                                "--norc",
                                "-e",
                                "-o",
                                "pipefail",
                                "-c",
                                script,
                            ],
                            cwd=root,
                            env=os.environ
                            | {
                                "PATH": f"{root / 'bin'}:{os.environ['PATH']}",
                                "CAPTURE_FILE": str(capture),
                                "RUNNER_OS": runner,
                                "RUNNER_TEMP": str(runner_temp),
                                "SNAPSHOT": str(snapshot).lower(),
                            },
                            capture_output=True,
                            text=True,
                        )
                        self.assertEqual(
                            result.returncode, 0, result.stdout + result.stderr
                        )
                        expected = [
                            "dev/sdk_version.py",
                            "materialize",
                            "--plan",
                            str(runner_temp / "sdk-release-plan/release-plan.json"),
                        ]
                        if runner != "Windows":
                            expected = [
                                "--shell",
                                "js-node",
                                "--command",
                                "python3.11",
                                *expected,
                            ]
                        if snapshot:
                            expected.append("--snapshot")
                        self.assertEqual(capture.read_text().splitlines(), expected)


if __name__ == "__main__":
    unittest.main()
