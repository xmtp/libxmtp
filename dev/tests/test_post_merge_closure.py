#!/usr/bin/env python3
"""Check the dry-run parser of the post-merge closure check.

The step accepts only the `nix build --dry-run` lines it knows, so a change
of that text in a new Nix fails every post-merge run. These cases hold the
text of the Nix versions that CI pins.
"""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
STEP = next(
    step
    for step in yaml.safe_load((ROOT / ".github/workflows/post-merge.yml").read_text())[
        "jobs"
    ]["closure"]["steps"]
    if step.get("name", "").startswith("Dry-run the set members")
)
# The parser part of the step: it reads $RUNNER_TEMP/dry-run.txt.
PARSER = STEP["run"][STEP["run"].index("header=") :]
PATH_A = "  /nix/store/l8si8gnvvq93yzms1jsgh5aixyf9rl5x-glibc-2.42-67"
PATH_B = "  /nix/store/c2h2f4cw9p8i8zcfy52fd1dd6g0yhnki-hello-2.12.3"


def parse(text):
    with tempfile.TemporaryDirectory() as temp:
        Path(temp, "dry-run.txt").write_text(text)
        return subprocess.run(
            ["bash", "-eo", "pipefail", "-c", PARSER],
            env=os.environ | {"RUNNER_TEMP": temp},
            capture_output=True,
            text=True,
        )


class DryRunParserTests(unittest.TestCase):
    def assert_accepts(self, header, *paths):
        result = parse("\n".join([header, *paths]) + "\n")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_nix_2_31_sizes(self):
        self.assert_accepts(
            "these 2 paths will be fetched (837.50 MiB download, 3194.02 MiB unpacked):",
            PATH_A,
            PATH_B,
        )

    def test_nix_2_34_sizes_pick_their_unit(self):
        # Nix 2.34 prints one decimal and switches the unit with the size.
        self.assert_accepts(
            "these 2 paths will be fetched (837.5 MiB download, 3.1 GiB unpacked):",
            PATH_A,
            PATH_B,
        )
        self.assert_accepts(
            "this path will be fetched (512.0 KiB download, 1.2 MiB unpacked):",
            PATH_A,
        )

    def test_a_path_to_build_fails(self):
        result = parse(
            "these 2 paths will be fetched (1.0 MiB download, 2.0 MiB unpacked):\n"
            f"{PATH_A}\n{PATH_B}\n"
            "this derivation will be built:\n"
            "  /nix/store/zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-x.drv\n"
        )
        self.assertNotEqual(result.returncode, 0)

    def test_a_count_mismatch_fails(self):
        result = parse(
            "these 3 paths will be fetched (1.0 MiB download, 2.0 MiB unpacked):\n"
            f"{PATH_A}\n{PATH_B}\n"
        )
        self.assertNotEqual(result.returncode, 0)


class ReactNativeProducerTests(unittest.TestCase):
    def test_cached_aggregate_still_realizes_raw_slices_before_components(self):
        workflow = yaml.safe_load(
            (ROOT / ".github/workflows/post-merge.yml").read_text()
        )
        steps = workflow["jobs"]["rn-components"]["steps"]
        base = next(
            step["run"]
            for step in steps
            if step.get("name", "").startswith("Build the reused Rust base")
        )
        components = next(
            step["run"]
            for step in steps
            if step.get("name") == "Build and export completed components"
        )
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            (directory / "bin").mkdir()
            (directory / "dev").mkdir()
            nix = directory / "bin/nix"
            nix.write_text("""#!/usr/bin/env bash
for arg in "$@"; do
  case "$arg" in
    *native-base) touch "$RN_RAW_SLICES" ;;
    *components) test -f "$RN_RAW_SLICES" || exit 41 ;;
  esac
done
""")
            nix.chmod(0o755)
            wrapper = directory / "dev/nix-shell"
            wrapper.write_text("#!/usr/bin/env bash\nexit 0\n")
            wrapper.chmod(0o755)
            env = os.environ | {
                "PATH": str(directory / "bin") + os.pathsep + os.environ["PATH"],
                "SYSTEM": "aarch64-darwin",
                "RN_RAW_SLICES": str(directory / "raw-slices"),
            }
            # The cached base aggregate does not restore intermediate raw
            # slices. Building components after switching Xcode needs them.
            for command in (base, components):
                result = subprocess.run(
                    ["bash", "-euc", command],
                    cwd=directory,
                    env=env,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
