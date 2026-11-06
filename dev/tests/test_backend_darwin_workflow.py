#!/usr/bin/env python3
"""Check the Darwin smoke uses the native service readiness lifecycle."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/test-backend-darwin.yml"


class DarwinBackendWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.workflow = yaml.safe_load(WORKFLOW.read_text())
        self.job = self.workflow["jobs"]["smoke"]

    def test_reusable_runner_and_optional_ci_cache_token(self):
        trigger = self.workflow.get("on", self.workflow.get(True))
        call = trigger["workflow_call"]
        self.assertEqual(call["inputs"]["runner"]["type"], "string")
        self.assertIs(call["inputs"]["runner"]["required"], True)
        self.assertIs(call["secrets"]["CACHIX_PR_AUTH_TOKEN"]["required"], False)
        self.assertEqual(self.job["runs-on"], "${{ inputs.runner }}")
        self.assertEqual(self.workflow["permissions"], {"contents": "read"})
        self.assertEqual(self.job["env"]["NIX_DEVSHELL"], "rust")
        setup = next(
            step
            for step in self.job["steps"]
            if step.get("uses") == "./.github/actions/setup-nix"
        )
        self.assertEqual(setup["with"]["cache-mode"], "ci")
        self.assertEqual(
            setup["with"]["cachix-pr-auth-token"],
            "${{ secrets.CACHIX_PR_AUTH_TOKEN }}",
        )
        self.assertFalse(any(key.startswith("kache") for key in setup["with"]))

    def test_service_logs_are_retained_even_after_failure(self):
        upload = next(
            step
            for step in self.job["steps"]
            if step.get("uses", "").startswith("actions/upload-artifact@")
        )
        self.assertEqual(upload["if"], "always()")
        self.assertEqual(upload["with"]["path"], "${{ runner.temp }}/backend-ci-logs")

    def run_smoke(self, status):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        names = subprocess.check_output(
            ["git", "ls-files", "*.just", "justfile"], cwd=ROOT, text=True
        ).splitlines()
        for name in names:
            target = root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, target)
        (root / "dev/docker").mkdir(parents=True, exist_ok=True)
        (root / "dev/docker/load-env").write_text("")
        wrapper = root / "dev/nix-shell"
        wrapper.write_text('#!/usr/bin/env bash\nexec bash -euc "$1"\n')
        wrapper.chmod(0o755)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        nix = bin_dir / "nix"
        nix.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, pathlib, subprocess, sys\n"
            "args = sys.argv[1:]\n"
            "assert args == ['run', os.getcwd() + '#services-test'], args\n"
            "pathlib.Path('invocation.json').write_text(json.dumps({\n"
            "    'command': os.environ['XMTP_SERVICES_TEST_COMMAND'],\n"
            "    'logs': os.environ['XMTP_SERVICES_LOGS'],\n"
            "}))\n"
            "status = int(os.environ['SERVICE_STATUS'])\n"
            "if status:\n"
            "    sys.exit(status)\n"
            "sys.exit(subprocess.call(os.environ['XMTP_SERVICES_TEST_COMMAND'], "
            "shell=True))\n"
        )
        nix.chmod(0o755)
        environment = dict(os.environ)
        environment.update(self.workflow.get("env", {}))
        environment.update(self.job.get("env", {}))
        environment.update(
            PATH=str(bin_dir) + os.pathsep + os.environ["PATH"],
            RUNNER_TEMP=str(root / "runner-temp"),
            SERVICE_STATUS=str(status),
        )
        environment.pop("XMTP_SERVICES_LOGS", None)
        commands = [step["run"] for step in self.job["steps"] if "run" in step]
        self.assertEqual(len(commands), 1)
        result = subprocess.run(
            ["bash", "-euc", commands[0]],
            cwd=root,
            env=environment,
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        invocation = json.loads((root / "invocation.json").read_text())
        self.assertEqual(invocation["command"].split(), ["true"])
        self.assertEqual(invocation["logs"], str(root / "runner-temp/backend-ci-logs"))

    def test_smoke_builds_and_starts_services_without_sdk_work(self):
        self.run_smoke(0)

    def test_smoke_propagates_service_readiness_failure(self):
        self.run_smoke(42)


if __name__ == "__main__":
    unittest.main()
