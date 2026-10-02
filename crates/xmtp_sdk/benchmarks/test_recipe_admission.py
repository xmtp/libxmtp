"""Recipe arguments must reach runner admission without shell evaluation."""

import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[3]


class RecipeAdmissionTests(unittest.TestCase):
    def test_benchmark_arguments_are_shell_literals(self):
        with tempfile.TemporaryDirectory(prefix="sdk-recipe-admission-") as directory:
            root = pathlib.Path(directory)
            for position in range(3):
                for form in ("substitution", "separator"):
                    with self.subTest(position=position, form=form):
                        marker = root / f"marker-{position}-{form}"
                        if form == "substitution":
                            payload = f"$(printf recipe-injected > {marker})"
                        else:
                            prefix = "node" if position == 0 else '"'
                            payload = (
                                f"{prefix} || printf recipe-injected > {marker}; #"
                            )
                        arguments = [
                            "node",
                            str(root / "absent-config.json"),
                            str(root / "output"),
                        ]
                        arguments[position] = payload
                        result = subprocess.run(
                            ["just", "sdk", "cutover-bench", *arguments],
                            cwd=ROOT,
                            text=True,
                            stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE,
                            timeout=15,
                        )
                        self.assertFalse(
                            marker.exists(),
                            f"recipe evaluated argument {position}: {form}",
                        )
                        self.assertNotEqual(
                            result.returncode, 0, "invalid runner input was accepted"
                        )


if __name__ == "__main__":
    unittest.main()
