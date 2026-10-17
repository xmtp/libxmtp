"""Exercise RN product handoffs through the production composite command."""

import fnmatch
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
ACTION = yaml.safe_load(
    (ROOT / ".github/actions/react-native-product/action.yml").read_text()
)
COMMAND = next(
    step["run"]
    for step in ACTION["runs"]["steps"]
    if step.get("name") == "Build and exchange the declared products"
)


def download_package(directory, archive):
    """Apply the downloader's single-artifact extraction contract to its inputs."""

    def expression(raw):
        raw = raw.replace("inputs.stage", repr("consumers"))
        raw = raw.replace("inputs.artifact-prefix", repr("same-run"))
        return eval(
            raw.replace("&&", " and ").replace("||", " or "), {"__builtins__": {}}, {}
        )

    artifacts = {"same-run-package-closure": archive}
    for step in ACTION["runs"]["steps"]:
        if not step.get("uses", "").startswith("actions/download-artifact@"):
            continue
        if not expression(step["if"]):
            continue
        inputs = {
            key: re.sub(
                r"\$\{\{\s*(.*?)\s*\}\}", lambda match: expression(match[1]), raw
            )
            for key, raw in step["with"].items()
        }
        pattern = inputs.get("name", inputs.get("pattern", "*"))
        selected = {
            name: path
            for name, path in artifacts.items()
            if fnmatch.fnmatchcase(name, pattern)
        }
        assert selected, pattern
        for name, path in selected.items():
            # v8 extracts a single match directly into `path`, even with pattern.
            destination = directory / inputs["path"]
            if len(selected) > 1:
                destination /= name
            shutil.copytree(path, destination)


class ReactNativeProductTests(unittest.TestCase):
    def test_default_and_release_handoffs_preserve_package_identity(self):
        for version in ("", "8.0.1-pre.20260102030405.nightly.abcdef0"):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                binary = directory / "bin"
                binary.mkdir()
                (directory / "dev").mkdir()
                sdk = directory / "sdks/react-native"
                sdk.mkdir(parents=True)
                (sdk / "package.json").write_text('{"version":"8.0.0"}')
                store = directory / "package"
                store.mkdir()
                env = os.environ | {
                    "PATH": str(binary) + os.pathsep + os.environ["PATH"],
                    "RN_SYSTEM": "x86_64-linux",
                    "RN_SOURCE_SHA": "a" * 40,
                    "RN_PACKAGE_VERSION": version,
                    "RN_ARTIFACT_PREFIX": "same-run",
                    "RN_CACHE_MODE": "read" if version else "ci",
                    "RUNNER_OS": "Linux",
                    "GITHUB_ENV": str(directory / "output"),
                    "RUNNER_TEMP": str(directory),
                    "EVENTS": str(directory / "events"),
                    "PACKAGE": str(store),
                }
                (binary / "git").write_text(
                    '#!/usr/bin/env bash\nprintf "%s\\n" "$RN_SOURCE_SHA"\n'
                )
                (binary / "nix").write_text("""#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with open(os.environ["EVENTS"], "a") as output:
    output.write(json.dumps(["nix", args, os.environ["RN_PACKAGE_VERSION"]]) + "\\n")
if args[0] == "build":
    print(json.dumps([{"outputs": {"out": "/nix/store/consumer-bundles"}}]))
elif "sdkProvenance.source" in " ".join(args):
    print("source-identity")
else:
    print("/nix/store/consumer-bundles")
""")
                wrapper = directory / "dev/nix-shell"
                wrapper.write_text("""#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
args = args[args.index("nix/react-native-exchange.py") + 1:]
with open(os.environ["EVENTS"], "a") as output:
    output.write(json.dumps(["exchange", args]) + "\\n")
directory = Path(args[args.index("--directory") + 1])
if args[0] == "export":
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "manifest.json").write_text(json.dumps({
        "roots": {"xmtp-sdk-react-native-package": os.environ["PACKAGE"]}
    }))
else:
    assert (directory / "manifest.json").is_file(), directory
""")
                for path in (*binary.iterdir(), wrapper):
                    path.chmod(0o755)
                for system in ("x86_64-linux", "aarch64-darwin"):
                    path = (
                        directory / f"target/rn-exchange/same-run-components-{system}"
                    )
                    path.mkdir(parents=True)
                    (path / "manifest.json").write_text("{}")
                self.run_stage(directory, env, "package")
                archive = directory / "uploaded-package"
                shutil.copytree(directory / "target/rn-exchange/package", archive)
                shutil.rmtree(directory / "target/rn-exchange")
                download_package(directory, archive)
                self.run_stage(directory, env, "consumers")
                events = [
                    json.loads(line)
                    for line in Path(env["EVENTS"]).read_text().splitlines()
                ]
                exchanges = [
                    args for command, args, *_ in events if command == "exchange"
                ]
                self.assertEqual(
                    [args[0] for args in exchanges],
                    ["import", "import", "export", "import"],
                )
                self.assertEqual(
                    [args[args.index("--system") + 1] for args in exchanges[:2]],
                    ["x86_64-linux", "aarch64-darwin"],
                )
                for args in exchanges[2:]:
                    self.assertNotIn("", args)
                    self.assertEqual(args[args.index("--stage") + 1], "package")
                    self.assertEqual(
                        args[args.index("--version") + 1]
                        if "--version" in args
                        else "",
                        version,
                    )
                builds = [
                    event
                    for event in events
                    if event[:1] == ["nix"] and event[1][0] == "build"
                ]
                self.assertEqual(len(builds), 2)
                self.assertTrue(all(event[2] == version for event in builds))
                self.assertTrue(
                    all(("--impure" in event[1]) == bool(version) for event in builds)
                )
                output = Path(env["GITHUB_ENV"]).read_text()
                self.assertIn(f"XMTP_RN_PACKAGE_DIR={store}\n", output)
                self.assertIn(
                    f"XMTP_RN_EXPECTED_VERSION={version or '8.0.0'}\n", output
                )
                self.assertIn(
                    "XMTP_RN_CONSUMER_BUNDLES=/nix/store/consumer-bundles\n", output
                )

    def run_stage(self, directory, env, stage):
        result = subprocess.run(
            ["/bin/bash", "-euc", COMMAND],
            cwd=directory,
            env=env | {"RN_STAGE": stage},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_wrong_checkout_is_rejected_before_building(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            git = directory / "git"
            git.write_text('#!/usr/bin/env bash\nprintf "%040d\\n" 0\n')
            git.chmod(0o755)
            nix = directory / "nix"
            nix.write_text('#!/usr/bin/env bash\ntouch "$BUILD_REACHED"\n')
            nix.chmod(0o755)
            (directory / "dev").mkdir()
            wrapper = directory / "dev/nix-shell"
            wrapper.write_text("#!/usr/bin/env bash\nexit 0\n")
            wrapper.chmod(0o755)
            result = subprocess.run(
                ["/bin/bash", "-euc", COMMAND],
                cwd=directory,
                env=os.environ
                | {
                    "PATH": str(directory) + os.pathsep + os.environ["PATH"],
                    "RN_SOURCE_SHA": "a" * 40,
                    "RN_ARTIFACT_PREFIX": "same-run",
                    "RN_CACHE_MODE": "read",
                    "RN_SYSTEM": "x86_64-linux",
                    "RN_STAGE": "components",
                    "RN_PACKAGE_VERSION": "",
                    "BUILD_REACHED": str(directory / "built"),
                },
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((directory / "built").exists())


if __name__ == "__main__":
    unittest.main()
