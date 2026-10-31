"""A clean publisher installs the Browser smoke's workspace dependency."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml

from test_react_native_workflow import expression

ROOT = Path(__file__).resolve().parents[2]


class BrowserReleaseTest(unittest.TestCase):
    def setup_step(self):
        steps = yaml.safe_load(
            (ROOT / ".github/workflows/npm-prepare.yml").read_text()
        )["jobs"]["prepare"]["steps"]
        setup = next(
            (
                step
                for step in steps
                if step.get("name") == "Set up Browser publication consumer"
            ),
            None,
        )
        self.assertIsNotNone(
            setup, "Clean publisher has no Browser Playwright dependency"
        )
        self.assertEqual(setup["uses"], "./.github/actions/setup-js")
        self.assertEqual(setup["with"]["cache-mode"], "read")
        check = next(
            step
            for step in steps
            if step.get("name") == "Verify the exact Browser publication archive"
        )
        self.assertLess(steps.index(setup), steps.index(check))
        return setup

    def test_clean_filtered_publisher_can_import_the_browser_smoke_dependency(self):
        setup = self.setup_step()
        action = yaml.safe_load(
            (ROOT / ".github/actions/setup-js/action.yml").read_text()
        )
        install = next(
            step["run"]
            for step in action["runs"]["steps"]
            if step.get("run", "").startswith(
                "dev/nix-shell --shell js-node 'pnpm install"
            )
        )
        tools = yaml.safe_load(
            (ROOT / ".github/actions/setup-release-tools/action.yml").read_text()
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)

            def write(path, data):
                file = root / path
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text(json.dumps(data))

            write(
                "package.json",
                {
                    "private": True,
                    "packageManager": json.loads((ROOT / "package.json").read_text())[
                        "packageManager"
                    ],
                },
            )
            write(
                "pnpm-workspace.yaml",
                {
                    "packages": [
                        "dev/release-tools",
                        "sdks/browser",
                        "packages/playwright",
                    ],
                    "includeWorkspaceRoot": False,
                },
            )
            write(
                "dev/release-tools/package.json",
                {"name": "@xmtp/release-tools", "version": "1.0.0"},
            )
            write(
                "sdks/browser/package.json",
                {
                    "name": "@xmtp/browser-sdk",
                    "version": "8.0.0",
                    "devDependencies": {"playwright": "workspace:*"},
                },
            )
            write(
                "packages/playwright/package.json",
                {
                    "name": "playwright",
                    "version": "1.61.1",
                    "type": "module",
                    "main": "index.mjs",
                },
            )
            (root / "packages/playwright/index.mjs").write_text(
                "export const dependencyLoaded = true;\n"
            )
            write(
                "pnpm-lock.yaml",
                {
                    "lockfileVersion": "9.0",
                    "settings": {
                        "autoInstallPeers": True,
                        "excludeLinksFromLockfile": False,
                    },
                    "importers": {
                        ".": {},
                        "dev/release-tools": {},
                        "packages/playwright": {},
                        "sdks/browser": {
                            "devDependencies": {
                                "playwright": {
                                    "specifier": "workspace:*",
                                    "version": "link:../../packages/playwright",
                                }
                            }
                        },
                    },
                },
            )
            wrapper = root / "dev/nix-shell"
            wrapper.write_text(
                '#!/bin/bash\nset -euo pipefail\nshift 2\nexec bash -euc "$1"\n'
            )
            wrapper.chmod(0o755)
            env = dict(
                os.environ,
                CI="true",
                pnpm_config_offline="true",
                pnpm_config_store_dir=str(root / "store"),
                PNPM_IGNORE_SCRIPTS="1",
            )
            result = subprocess.run(
                ["bash", "-euc", install],
                cwd=root,
                env=dict(env, PNPM_FILTER=tools["inputs"]["filter"]["default"]),
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            module = root / "sdks/browser/node_modules/playwright/index.mjs"
            self.assertFalse(module.exists())
            result = subprocess.run(
                ["bash", "-euc", install],
                cwd=root,
                env=dict(env, PNPM_FILTER=setup["with"]["filter"]),
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = subprocess.run(
                [
                    "node",
                    "--input-type=module",
                    "-e",
                    "import {dependencyLoaded} from './sdks/browser/node_modules/playwright/index.mjs'; if (!dependencyLoaded) process.exit(1)",
                ],
                cwd=root,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_browser_dependency_setup_is_only_a_readonly_preparation_step(self):
        setup = self.setup_step()
        for sdk in ("browser-sdk", "node-sdk", "agent-sdk", "cli", "react-native-sdk"):
            for prepare in (False, True):
                self.assertEqual(
                    expression(
                        setup["if"], {"inputs.sdk": sdk, "inputs.prepare-only": prepare}
                    ),
                    sdk == "browser-sdk" and prepare,
                )


if __name__ == "__main__":
    unittest.main()
