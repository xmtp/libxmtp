"""Fast release contracts; no SDK builds, installs, services or publication."""

import base64
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
SDKS = (
    "ios",
    "android",
    "node-sdk",
    "browser-sdk",
    "agent-sdk",
    "react-native-sdk",
    "cli",
)
PACKAGES = {
    "node-sdk": "sdks/node/package.json",
    "browser-sdk": "sdks/browser/package.json",
    "agent-sdk": "sdks/agent/package.json",
    "react-native-sdk": "sdks/react-native/package.json",
    "cli": "apps/cli/package.json",
}
REGISTRY = "https://registry.npmjs.org/"


class ReleaseValidation(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="release-validation-")
        self.addCleanup(temporary.cleanup)
        self.scratch = Path(temporary.name)
        self.checkout = self.scratch / "checkout"
        files = [
            "Cargo.toml",
            "Cargo.lock",
            "sdks/android/gradle.properties",
            "dev/sdk_version.py",
            *PACKAGES.values(),
        ]
        files += [
            str(p.relative_to(ROOT))
            for parent in ("apps", "crates")
            for p in (ROOT / parent).glob("*/Cargo.toml")
        ]
        for name in files:
            destination = self.checkout / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, destination)
        launcher = self.checkout / "dev/nix-shell"
        launcher.write_text('#!/bin/sh\nshift 3\nexec "$@"\n')
        launcher.chmod(0o755)
        self.env = {
            k: v
            for k, v in os.environ.items()
            if not k.startswith("GIT_") and k != "GITHUB_STATE"
        }
        self.env.update(
            GIT_CONFIG_GLOBAL="/dev/null",
            GIT_CONFIG_NOSYSTEM="1",
            RUNNER_TEMP=str(self.scratch),
        )
        self.git("init", "--quiet")
        self.git("config", "user.name", "release-fixture")
        self.git("config", "user.email", "release-fixture@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "checked-in release metadata")
        self.sha = self.git("rev-parse", "HEAD").stdout.strip()
        self.target = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"][
            "package"
        ]["version"]
        self.workflow = yaml.safe_load(
            (ROOT / ".github/workflows/release.yml").read_text()
        )

    def command(self, *args, env=None, error=None):
        result = subprocess.run(
            args,
            cwd=self.checkout,
            env=self.env | (env or {}),
            capture_output=True,
            text=True,
        )
        if error is None:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn(error, result.stdout + result.stderr)
        return result

    def git(self, *args):
        return self.command("git", *args)

    def version(self, *args, error=None):
        return self.command(
            sys.executable, "-B", "dev/sdk_version.py", *args, error=error
        )

    def step(self, step, env=None, error=None):
        return self.command("bash", "-euc", step["run"], env=env, error=error)

    def test_shared_planning_materialization_and_retained_identity(self):
        self.assertEqual(self.version("check").stdout.strip(), self.target)
        steps = self.workflow["jobs"]["validate"]["steps"]
        planner = next(step for step in steps if step.get("id") == "versions")
        plans = {}
        for kind, suffix in (
            ("dev", "-pre.20261103091500.dev.g" + self.sha[:7]),
            ("rc", "-rc5" if self.target == "8.0.0" else "-rc.5"),
            ("final", ""),
        ):
            output = self.scratch / (kind + ".outputs")
            self.step(
                planner,
                {
                    "RELEASE_TYPE": kind,
                    "RC_NUMBER": "5" if kind == "rc" else "",
                    "TIMESTAMP": "20261103091500",
                    "GITHUB_OUTPUT": str(output),
                },
            )
            expected = self.target + suffix
            versions = dict(
                line.split("=", 1) for line in output.read_text().splitlines()
            )
            self.assertEqual(versions, {"version-" + sdk: expected for sdk in SDKS})
            record = json.loads((self.scratch / "release-plan.json").read_text())
            self.assertEqual(record["sourceSha"], self.sha)
            self.assertEqual(record["version"], expected)
            self.assertEqual(record["targetVersion"], self.target)
            path = self.scratch / (kind + ".json")
            path.write_text(json.dumps(record))
            plans[kind] = (path, record)
        plan, record = plans["rc"]
        wrong = self.scratch / "wrong.json"
        wrong.write_text(json.dumps(record | {"sourceSha": "0" * 40}))
        self.version("materialize", "--plan", str(wrong), error="frozen release source")
        tracked = self.checkout / "dev/sdk_version.py"
        original = tracked.read_bytes()
        tracked.write_bytes(original + b"\n")
        self.version("materialize", "--plan", str(plan), error="changed tracked source")
        tracked.write_bytes(original)
        self.version("materialize", "--plan", str(plan))
        self.version("materialize", "--plan", str(plan))
        self.assertEqual(self.version("check").stdout.strip(), record["version"])
        for sdk, name in PACKAGES.items():
            package = json.loads((self.checkout / name).read_text())
            self.assertEqual(package["version"], record["version"], sdk)
        android = (
            (self.checkout / "sdks/android/gradle.properties").read_text().splitlines()
        )
        self.assertIn("version=" + record["version"], android)
        cargo = tomllib.loads((self.checkout / "Cargo.toml").read_text())
        self.assertEqual(cargo["workspace"]["package"]["version"], record["version"])
        sdk_crate = tomllib.loads(
            (self.checkout / "crates/xmtp_sdk/Cargo.toml").read_text()
        )
        self.assertEqual(sdk_crate["package"]["version"], {"workspace": True})
        for sdk in SDKS:
            product = self.scratch / sdk
            product.mkdir()
            payload = product / "checked-product"
            payload.write_bytes((sdk + ":" + record["version"]).encode())
            self.version("seal", str(product), "--plan", str(plan))
            self.version("verify", str(product), "--plan", str(plan))
        self.version(
            "verify", str(product), "--plan", str(wrong), error="different release"
        )
        payload.write_bytes(b"corrupt")
        self.version(
            "verify", str(product), "--plan", str(plan), error="checksum mismatch"
        )
        drift = self.checkout / PACKAGES["browser-sdk"]
        package = json.loads(drift.read_text())
        drift.write_text(json.dumps(package | {"version": "0.0.0"}))
        self.version("check", error="sdks/browser/package.json")

    def test_selection_all_six_barrier_and_publication_permissions(self):
        jobs = self.workflow["jobs"]
        resolver = next(
            step for step in jobs["validate"]["steps"] if step.get("id") == "resolve"
        )
        gate = jobs["sdk-ready"]["steps"][0]
        for selected in SDKS:
            output = self.scratch / (selected + ".outputs")
            env = {
                "INPUT_" + sdk.upper().replace("-SDK", "_SDK").replace("-", "_"): str(
                    sdk == selected
                ).lower()
                for sdk in SDKS
            }
            env.update(
                INPUT_RELEASE_TYPE="rc",
                INPUT_REF="release/8.0.0",
                INPUT_BACKEND="false",
                INPUT_DRY_RUN="false",
                GITHUB_REF="refs/heads/main",
                GITHUB_OUTPUT=str(output),
            )
            self.step(resolver, env)
            selection = dict(
                line.split("=", 1) for line in output.read_text().splitlines()
            )
            self.assertEqual(selection["products-selected"], "true")
            for sdk in SDKS:
                expected = sdk == selected or (
                    sdk == "node-sdk" and selected in ("agent-sdk", "cli")
                )
                self.assertEqual(selection[sdk], str(expected).lower())
            outcomes = {"validate": {"result": "success"}} | {
                "build-" + sdk: {"result": "success"} for sdk in SDKS
            }
            env = {
                "SDK_SELECTION": json.dumps(selection),
                "JOB_RESULTS": json.dumps(outcomes),
                "SDK_JOB_PREFIX": "build",
            }
            self.step(gate, env)
            if selected == "cli":
                for failed in SDKS:
                    broken = outcomes | {"build-" + failed: {"result": "skipped"}}
                    self.step(
                        gate,
                        env | {"JOB_RESULTS": json.dumps(broken)},
                        error="Required product " + failed,
                    )
        for sdk in SDKS:
            self.assertIn("sdk-ready", jobs["release-" + sdk]["needs"])

        def permissions(name, granted):
            document = yaml.safe_load((ROOT / ".github/workflows" / name).read_text())
            defaults = document.get("permissions", granted)
            for job_id, job in document["jobs"].items():
                actual = job.get("permissions", defaults)
                self.assertNotEqual(
                    actual.get("id-token"), "write", name + "/" + job_id
                )
                called = job.get("uses", "")
                if called.startswith("./.github/workflows/"):
                    permissions(Path(called).name, actual)

        for sdk in PACKAGES:
            caller = jobs["build-" + sdk]
            self.assertNotEqual(caller["permissions"].get("id-token"), "write")
            permissions(Path(caller["uses"]).name, caller["permissions"])
        publisher = yaml.safe_load(
            (ROOT / ".github/workflows/npm-publish.yml").read_text()
        )
        self.assertEqual(
            publisher["jobs"]["publish"]["permissions"]["id-token"], "write"
        )
        self.assertNotIn("id-token", publisher["jobs"]["push-tag"]["permissions"])

    def test_npm_checked_metadata_and_canonical_destination(self):
        binary = self.scratch / "bin"
        binary.mkdir()
        npm = binary / "npm"
        npm.write_text(
            f"#!{sys.executable}\n"
            + """import json, os, sys
from pathlib import Path
args = sys.argv[1:]
with Path(os.environ["CALLS"]).open("a") as log:
    log.write(json.dumps(args) + "\\n")
if args[0] == "view" and args[2] == "version":
    print(json.dumps(os.environ["VERSION"]))
elif args[0] == "view":
    if os.environ.get("INTEGRITY"):
        print(json.dumps(os.environ["INTEGRITY"]))
    else:
        print(json.dumps({"error": {"code": "E404"}}))
        sys.exit(1)
elif args[0] != "publish":
    sys.exit("unexpected npm command")
"""
        )
        npm.chmod(0o755)
        calls = self.scratch / "npm.jsonl"
        env = {
            "PATH": str(binary) + os.pathsep + self.env["PATH"],
            "CALLS": str(calls),
            "VERSION": self.target,
            "INTEGRITY": "",
            "NPM_CONFIG_REGISTRY": "https://ignored.example.invalid/",
        }
        archive = self.scratch / "package.tgz"

        def pack(package):
            payload = json.dumps(package).encode()
            with tarfile.open(archive, "w:gz") as output:
                entry = tarfile.TarInfo("package/package.json")
                entry.size = len(payload)
                output.addfile(entry, io.BytesIO(payload))
            calls.write_text("")

        def publish(sdk, error=None, extra=None):
            return self.command(
                sys.executable,
                "-B",
                str(ROOT / "dev/publish_npm.py"),
                str(archive),
                self.target,
                "latest",
                sdk,
                env=env | (extra or {}),
                error=error,
            )

        for sdk, name in PACKAGES.items():
            package = json.loads((ROOT / name).read_text())
            if sdk in ("agent-sdk", "cli"):
                self.assertEqual(
                    package["dependencies"]["@xmtp/node-sdk"], "workspace:*"
                )
                package["dependencies"]["@xmtp/node-sdk"] = self.target
            pack(package)
            publish(sdk)
            commands = [json.loads(line) for line in calls.read_text().splitlines()]
            self.assertEqual(sum(args[0] == "publish" for args in commands), 1)
            for args in commands:
                self.assertEqual(args[args.index("--registry") + 1], REGISTRY)
                self.assertIn("--@xmtp:registry=" + REGISTRY, args)
            integrity = (
                "sha512-"
                + base64.b64encode(
                    hashlib.sha512(archive.read_bytes()).digest()
                ).decode()
            )
            calls.write_text("")
            publish(sdk, extra={"INTEGRITY": integrity})
            self.assertNotIn(
                "publish",
                [json.loads(line)[0] for line in calls.read_text().splitlines()],
            )
            publish(
                sdk, error="different bytes", extra={"INTEGRITY": "sha512-conflict"}
            )
            for field in ("registry", "@xmtp:registry"):
                config = {
                    "registry": REGISTRY,
                    "@xmtp:registry": REGISTRY,
                    field: "https://rejected.example.invalid/",
                }
                pack(package | {"publishConfig": config})
                publish(sdk, error="registry")
                self.assertEqual(calls.read_text(), "")


if __name__ == "__main__":
    unittest.main()
