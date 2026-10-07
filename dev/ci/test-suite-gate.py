#!/usr/bin/env python3
"""Exercise selected suite and required platform gates at their real entry points."""

import json
import ast
import os
import re
from pathlib import Path
import subprocess
import sys
import unittest
import tempfile
import itertools

import yaml

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ["lint_workspace", "lint_js", "lint_config", "lint_proto"]
TESTS = [
    "test_native_backend",
    "test_validation",
    "test_backend",
    "test_workspace",
    "test_keepalive",
    "test_wasm",
    "test_node",
    "test_agent",
    "test_xdbg",
    "test_browser",
    "test_bindings",
    "test_sdk_staging",
    "test_bridge_runtime",
    "test_browser_platform",
]


class SuiteGateTests(unittest.TestCase):
    def condition(self, expression, prepared, kind, arch="amd64"):
        expression = expression.strip().removeprefix("${{").removesuffix("}}")
        expression = expression.replace(
            "inputs.prepared-products", repr(prepared)
        ).replace("matrix.kind", repr(kind))
        expression = expression.replace("matrix.arch", repr(arch)).replace(
            "always()", "True"
        )
        expression = (
            expression.replace("&&", " and ")
            .replace("||", " or ")
            .replace("!", " not ")
            .strip()
        )
        tree = ast.parse(expression, mode="eval")
        allowed = (
            ast.Expression,
            ast.BoolOp,
            ast.And,
            ast.Or,
            ast.UnaryOp,
            ast.Not,
            ast.Compare,
            ast.Eq,
            ast.Constant,
        )
        self.assertTrue(all(isinstance(node, allowed) for node in ast.walk(tree)))
        return eval(compile(tree, "workflow condition", "eval"), {"__builtins__": {}})

    def test_node_and_cli_matrix_retains_each_existing_command_and_shard(self):
        data = yaml.safe_load(
            (ROOT / ".github/workflows/test-node-sdk.yml").read_text()
        )
        self.assertEqual(set(data["jobs"]), {"test"})
        job = data["jobs"]["test"]
        self.assertTrue(job["strategy"]["fail-fast"])
        choices = re.findall("'([^']+)'", job["strategy"]["matrix"]["include"])
        prepared, fallback = map(json.loads, choices)
        self.assertEqual(
            prepared, [{"kind": "node", "shard": 1}, {"kind": "cli", "shard": 1}]
        )
        self.assertEqual(
            fallback, [{"kind": "node", "shard": 1}, {"kind": "node", "shard": 2}]
        )
        expected = {
            (True, "node"): [
                "dev/nix-shell 'just js test-node-sdk-prepared --exclude test/streamRecovery.test.ts'"
            ],
            (True, "cli"): ["dev/nix-shell 'just cli test-ci'"],
            (False, "node"): [
                "dev/nix-shell 'just js test-node-sdk-ci --shard ${{ matrix.shard }}/2 --exclude test/streamRecovery.test.ts'",
                "dev/nix-shell 'just cli test-ci --shard ${{ matrix.shard }}/2'",
            ],
        }
        for mode, rows in ((True, prepared), (False, fallback)):
            for row in rows:
                commands = [
                    step["run"]
                    for step in job["steps"]
                    if step.get("name", "").startswith(
                        ("Run node-sdk", "Run all node-sdk", "Run CLI", "Run all CLI")
                    )
                    and self.condition(step.get("if", "True"), mode, row["kind"])
                ]
                self.assertEqual(commands, expected[(mode, row["kind"])])

    def test_backend_matrix_retains_native_and_both_manual_image_architectures(self):
        data = yaml.safe_load((ROOT / ".github/workflows/test-backend.yml").read_text())
        self.assertEqual(set(data["jobs"]), {"test"})
        job = data["jobs"]["test"]
        self.assertTrue(job["strategy"]["fail-fast"])
        manual, automatic = map(
            json.loads,
            [
                value
                for value in re.findall(
                    "'([^']+)'", job["strategy"]["matrix"]["include"]
                )
                if value.startswith("[")
            ],
        )
        self.assertEqual(
            automatic,
            [{"kind": "native", "arch": "amd64"}, {"kind": "image", "arch": "amd64"}],
        )
        self.assertEqual(manual, automatic + [{"kind": "image", "arch": "arm64"}])
        commands = {}
        for row in manual:
            commands[(row["kind"], row["arch"])] = {
                step["name"]: step["run"]
                for step in job["steps"]
                if "run" in step
                and self.condition(
                    step.get("if", "True"), True, row["kind"], row["arch"]
                )
            }
        native = commands[("native", "amd64")]
        self.assertEqual(native["Check SQL cache"], "just backend sql-check")
        self.assertEqual(
            native["Test backend, replica recovery, and HTTPS streaming"],
            "just backend test",
        )
        self.assertEqual(native["Stop backend database"], "just backend db-down")
        self.assertNotIn("Build backend image", native)
        for arch in ("amd64", "arm64"):
            image = commands[("image", arch)]
            self.assertEqual(image["Build backend image"], "just backend image")
            self.assertEqual(
                image["Load backend image"], "docker load --input result-backend-image"
            )
            self.assertIn("grpc-health-probe", image["Run backend image"])
            self.assertNotIn(
                "Test backend, replica recovery, and HTTPS streaming", image
            )
            self.assertEqual("Verify observability" in image, arch == "amd64")

    def call(self, kind, checks, results, suites=None, row=""):
        command = [
            sys.executable,
            "-B",
            str(ROOT / "dev/ci/suite-gate.py"),
            "--kind",
            kind,
            "--selection",
            json.dumps({"schema_version": 1, "checks": checks}),
            "--needs",
            json.dumps(results),
        ]
        if suites is not None:
            command += ["--suites", json.dumps(suites)]
        if row:
            command += ["--row", row]
        return subprocess.run(command, capture_output=True, text=True)

    def test_each_selected_router_result_must_succeed(self):
        for kind, inventory in (("source", SOURCE), ("tests", TESTS)):
            checks = dict.fromkeys(inventory, True)
            for row in inventory:
                for result in ("success", "failure", "cancelled", "skipped", None):
                    with self.subTest(kind=kind, row=row, result=result):
                        actual = self.call(
                            kind, checks, {row: {"result": result}}, row=row
                        )
                        self.assertEqual(
                            actual.returncode == 0, result == "success", actual.stderr
                        )

    def test_matrix_selection_matches_every_required_row(self):
        checks = dict.fromkeys(TESTS, True)
        results = {"suites": {"result": "success"}}
        self.assertEqual(self.call("tests", checks, results, TESTS).returncode, 0)
        for rows in (TESTS[:-1], TESTS + [TESTS[0]], TESTS + ["unknown"], []):
            self.assertNotEqual(self.call("tests", checks, results, rows).returncode, 0)

    def test_failed_cancelled_skipped_or_missing_matrix_never_passes_selected(self):
        checks = dict.fromkeys(SOURCE, False)
        checks["lint_js"] = True
        for result in ("failure", "cancelled", "skipped", None):
            self.assertNotEqual(
                self.call("source", checks, {"suites": {"result": result}}).returncode,
                0,
            )
        self.assertNotEqual(self.call("source", checks, {}).returncode, 0)

    def test_intentionally_empty_matrix_and_legacy_subset(self):
        checks = dict.fromkeys(SOURCE, False)
        self.assertEqual(
            self.call(
                "source", checks, {"suites": {"result": "skipped"}}, []
            ).returncode,
            0,
        )
        checks["lint_proto"] = True
        self.assertEqual(
            self.call("source", checks, {"suites": {"result": "success"}}).returncode, 0
        )
        self.assertNotEqual(
            self.call(
                "source", checks, {"lint_js": {"result": "success"}}, row="lint_js"
            ).returncode,
            0,
        )

    def test_missing_nonboolean_and_unknown_rows_fail(self):
        checks = dict.fromkeys(SOURCE, False)
        for value in (None, "true", 1):
            bad = dict(checks, lint_js=value)
            self.assertNotEqual(
                self.call("source", bad, {"suites": {"result": "skipped"}}).returncode,
                0,
            )
        checks.pop("lint_js")
        self.assertNotEqual(
            self.call("source", checks, {"suites": {"result": "skipped"}}).returncode, 0
        )

    def test_matrix_aggregate_and_binding_platform_union_must_match(self):
        checks = dict.fromkeys(SOURCE, False)
        checks["source_lint"] = True
        self.assertNotEqual(
            self.call("source", checks, {"suites": {"result": "skipped"}}).returncode, 0
        )
        checks = dict.fromkeys(TESTS, False)
        checks.update(
            test_bindings=True, check_bindings_ios=False, check_bindings_android=False
        )
        self.assertNotEqual(
            self.call("tests", checks, {"suites": {"result": "success"}}).returncode, 0
        )
        checks["check_bindings_ios"] = True
        self.assertEqual(
            self.call("tests", checks, {"suites": {"result": "success"}}).returncode, 0
        )

    def test_fixed_routers_and_matrix_policies_keep_the_current_inventory(self):
        for wrapper, router, inventory in (
            ("lint.yml", "lint-target.yml", SOURCE),
            ("test.yml", "test-target.yml", TESTS),
        ):
            data = yaml.safe_load((ROOT / ".github/workflows" / wrapper).read_text())
            self.assertTrue(data["jobs"]["suites"]["strategy"]["fail-fast"])
            self.assertEqual(
                data["jobs"]["suites"]["uses"], "./.github/workflows/" + router
            )
            target = yaml.safe_load((ROOT / ".github/workflows" / router).read_text())
            self.assertEqual(set(target["jobs"]), set(inventory) | {"result"})
            for key in inventory:
                self.assertTrue(
                    target["jobs"][key]["uses"].startswith("./.github/workflows/")
                )
                self.assertNotIn("${{", target["jobs"][key]["uses"])


class RequiredCIGateTests(unittest.TestCase):
    def actual_gate(self, job, checks, changes):
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        data = workflow["jobs"][job]
        needs = {name: {"result": "skipped"} for name in data["needs"]}
        needs["detect-changes"]["result"] = "success"
        needs.update(changes)
        body = next(step["run"] for step in data["steps"] if "run" in step)
        env = dict(
            os.environ,
            CI_SELECTION=json.dumps({"schema_version": 1, "checks": checks}),
            CI_NEEDS=json.dumps(needs),
            DOCS_SITE="true" if checks.get("docs_site") else "false",
        )
        return subprocess.run(
            ["bash", "-e", "-o", "pipefail", "-c", body],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
        )

    def test_selected_mobile_lint_and_tests_block_actual_required_gate(self):
        jobs = {
            "lint": {
                "source_lint": "source-lint",
                "docs_quality": "docs-quality",
                "lint_ios": "lint-ios",
                "lint_android": "lint-android",
            },
            "test": {
                "sdk_node": "sdk-node",
                "sdk_browser": "sdk-browser",
                "backend_products": "backend-products",
                "check_rust": "check-rust",
                "check_types": "check-types",
                "check_sdk": "check-sdk",
                "check_sdk_unit": "check-sdk-unit",
                "tests": "tests",
                "docs_site": "docs",
                "docs_rust": "docs-rust-reference",
                "test_ios": "test-ios",
                "test_ios_platform": "test-ios",
                "test_android": "test-android",
                "test_android_consumers": "test-android",
                "test_android_platform": "test-android",
                "test_swift_seams": "test-swift-seams",
            },
        }
        for job, mapping in jobs.items():
            mobile = [
                key
                for key in mapping
                if key.startswith(
                    ("lint_ios", "lint_android", "test_ios", "test_android")
                )
            ]
            for key in mobile:
                checks = dict.fromkeys(mapping, False)
                checks[key] = True
                for result in ("success", "failure", "cancelled", "skipped"):
                    with self.subTest(job=job, key=key, result=result):
                        actual = self.actual_gate(
                            job, checks, {mapping[key]: {"result": result}}
                        )
                        self.assertEqual(
                            actual.returncode == 0, result == "success", actual.stderr
                        )
                checks[key] = False
                self.assertEqual(self.actual_gate(job, checks, {}).returncode, 0)

    def test_standalone_rust_docs_and_full_site_routes_are_both_required(self):
        keys = [
            "sdk_node",
            "sdk_browser",
            "backend_products",
            "check_rust",
            "check_types",
            "check_sdk",
            "check_sdk_unit",
            "tests",
            "docs_site",
            "docs_rust",
            "test_ios",
            "test_ios_platform",
            "test_android",
            "test_android_consumers",
            "test_android_platform",
            "test_swift_seams",
        ]
        for site in (False, True):
            checks = dict.fromkeys(keys, False)
            checks.update(docs_site=site, docs_rust=True)
            job = "docs" if site else "docs-rust-reference"
            for result in ("success", "failure", "cancelled", "skipped"):
                actual = self.actual_gate("test", checks, {job: {"result": result}})
                self.assertEqual(
                    actual.returncode == 0, result == "success", actual.stderr
                )


class SDKOwnerRoutingTests(unittest.TestCase):
    def workflow(self, name):
        return yaml.safe_load((ROOT / ".github/workflows" / name).read_text())

    def test_migrated_linux_owners_have_distinct_required_matrix_rows(self):
        wrapper = self.workflow("test.yml")["jobs"]["suites"]
        router = self.workflow("test-target.yml")["jobs"]
        self.assertEqual(len(TESTS), 14)
        for key, target in (
            ("test_bridge_runtime", "bridge"),
            ("test_browser_platform", "browser"),
        ):
            self.assertIn("checks." + key, wrapper["if"])
            self.assertIn('"' + key + '"', wrapper["strategy"]["matrix"]["suite"])
            self.assertEqual(
                router[key]["uses"], "./.github/workflows/test-sdk-platform.yml"
            )
            self.assertEqual(router[key]["with"]["target"], target)
            self.assertIn("checks." + key, router[key]["if"])
            self.assertIn(key, router["result"]["needs"])
            checks = dict.fromkeys(TESTS, False)
            checks[key] = True
            for result in ("success", "failure", "cancelled", "skipped", None):
                actual = SuiteGateTests().call(
                    "tests", checks, {key: {"result": result}}, row=key
                )
                self.assertEqual(
                    actual.returncode == 0, result == "success", actual.stderr
                )

    def test_platform_job_restores_products_and_runs_exact_owner_recipes(self):
        job = self.workflow("test-sdk-platform.yml")["jobs"]["proof"]
        self.assertEqual(job["env"] if "env" in job else {}, {})
        steps = job["steps"]
        sdk = [
            step
            for step in steps
            if step.get("uses") == "./.github/actions/sdk-product"
        ]
        self.assertEqual([step["with"]["target"] for step in sdk], ["node", "browser"])
        self.assertTrue(all(step["if"] == "inputs.prepared-products" for step in sdk))
        self.assertTrue(
            any(
                step.get("uses") == "./.github/actions/backend-product"
                for step in steps
            )
        )
        commands = {step.get("name"): step for step in steps if "run" in step}
        for name, target, recipe in (
            ("Bridge runtime unit tests", "bridge", "test-bridge"),
            ("Browser platform proofs", "browser", "test-browser"),
        ):
            self.assertEqual(
                commands[name]["run"], "dev/nix-shell 'just sdk " + recipe + "'"
            )
            self.assertEqual(commands[name]["if"], "inputs.target == '" + target + "'")
        generator = commands["Build the fixture generator"]
        self.assertEqual(
            generator["if"], "inputs.prepared-products && inputs.target == 'browser'"
        )
        self.assertEqual(generator["env"]["NIX_DEVSHELL"], "rust")
        self.assertIn(
            "cargo build --locked -p xmtp-sdk-bindgen --target-dir target/sdk-fixture-tools",
            generator["run"],
        )
        self.assertIn("target/sdk-artifacts/bindgen/xmtp-sdk-bindgen", generator["run"])
        self.assertEqual(
            commands["Generate local SDK products"]["if"],
            "${{ !inputs.prepared-products }}",
        )
        self.assertEqual(commands["Stop test services"]["if"], "always()")
        self.assertEqual(commands["Stop test services"]["timeout-minutes"], 2)

    def test_browser_fixture_features_and_profiles_keep_upstream_contract(self):
        helpers = ROOT / "crates/xmtp_sdk/dev"
        panic = (helpers / "prepare-bridge-panic-fixture").read_text()
        pure = (helpers / "prepare-pure-codec-fixture").read_text()
        browser = (helpers / "run-browser-platform-tests").read_text()
        self.assertIn("--features bridge-panic-test,conformance", panic)
        self.assertNotIn("--release", panic)
        self.assertIn("build/wasm32-unknown-unknown/debug/xmtp_sdk.wasm", panic)
        self.assertIn("cargo build --locked --release -p xmtp_sdk", pure)
        self.assertIn("--features pure-only,conformance", pure)
        self.assertIn("build/wasm32-unknown-unknown/release/xmtp_sdk.wasm", pure)
        for name in ("prepare-bridge-panic-fixture", "prepare-pure-codec-fixture"):
            self.assertIn(
                "NIX_DEVSHELL=rust dev/nix-shell 'bash crates/xmtp_sdk/dev/"
                + name
                + "'",
                browser,
            )
        self.assertIn("sdks/browser/test/platform", browser)
        self.assertIn(
            "--browser.enabled=false", (helpers / "run-bridge-tests").read_text()
        )

    def test_swift_seams_preserve_native_commands_fork_rule_and_early_route(self):
        ci = self.workflow("ci.yml")["jobs"]
        route = ci["test-swift-seams"]
        self.assertEqual(route["needs"], "detect-changes")
        self.assertEqual(route["uses"], "./.github/workflows/test-swift-seams.yml")
        self.assertIn("checks.test_swift_seams", route["if"])
        fork_rule = "github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository"
        self.assertIn(fork_rule, route["if"])
        job = self.workflow("test-swift-seams.yml")["jobs"]["swift"]
        self.assertIn(fork_rule, job["if"])
        self.assertEqual(job["env"]["NIX_DEVSHELL"], "ios")
        self.assertEqual(job["runs-on"], "warp-macos-15-arm64-6x")
        steps = job["steps"]
        self.assertEqual(
            [step["run"] for step in steps if "run" in step],
            [
                "dev/nix-shell 'bash dev/check-apple-toolchain'",
                "dev/nix-shell 'just backend ci just ios test-seams'",
                "dev/nix-shell 'just ios check-consumer'",
            ],
        )
        self.assertFalse(
            any(step.get("uses") == "./.github/actions/setup-js" for step in steps)
        )
        self.assertEqual(
            next(
                step
                for step in steps
                if step.get("uses") == "maxim-lobanov/setup-xcode@v1"
            )["with"]["xcode-version"],
            "26",
        )
        self.assertIn("test-swift-seams", ci["test"]["needs"])
        mapping_keys = [
            "sdk_node",
            "sdk_browser",
            "backend_products",
            "check_rust",
            "check_types",
            "check_sdk",
            "check_sdk_unit",
            "tests",
            "docs_site",
            "docs_rust",
            "test_ios",
            "test_ios_platform",
            "test_android",
            "test_android_consumers",
            "test_android_platform",
            "test_swift_seams",
        ]
        checks = dict.fromkeys(mapping_keys, False)
        checks["test_swift_seams"] = True
        gate = RequiredCIGateTests()
        for result in ("success", "failure", "cancelled", "skipped", None):
            actual = gate.actual_gate(
                "test", checks, {"test-swift-seams": {"result": result}}
            )
            self.assertEqual(actual.returncode == 0, result == "success", actual.stderr)
        self.assertNotEqual(gate.actual_gate("test", checks, {}).returncode, 0)
        ios = self.workflow("test-ios.yml")["jobs"]
        commands = [
            step["run"]
            for job in ios.values()
            for step in job.get("steps", [])
            if "run" in step
        ]
        self.assertTrue(
            any(
                "just ios test skip-seams && just ios check-examples" in command
                for command in commands
            )
        )

    def test_android_broad_unit_job_retains_consumer_owner(self):
        steps = self.workflow("test-android.yml")["jobs"]["unit-tests"]["steps"]
        consumer_step = next(
            step for step in steps if step.get("name") == "Check Kotlin consumers"
        )
        self.assertEqual(
            consumer_step["run"], "dev/nix-shell 'just android check-consumers'"
        )
        self.assertEqual(consumer_step["if"], "inputs.run-consumers")
        staging = self.workflow("test-sdk-staging.yml")["jobs"]["android-stage"][
            "steps"
        ]
        self.assertFalse(
            any("check-consumers" in step.get("run", "") for step in staging)
        )
        consumer = (ROOT / "sdks/android/dev/check-consumers").read_text()
        self.assertIn("ConsumerNegative.kt", consumer)
        self.assertIn("CodecTypeNegative.kt", consumer)


class MobilePlatformSplitTests(unittest.TestCase):
    def workflow(self, name):
        return yaml.safe_load((ROOT / ".github/workflows" / name).read_text())

    def active(self, expression, flags):
        for key, value in flags.items():
            expression = expression.replace("inputs." + key, repr(value))
        expression = expression.replace("&&", " and ").replace("||", " or ")
        tree = ast.parse(expression, mode="eval")
        allowed = (ast.Expression, ast.BoolOp, ast.And, ast.Or, ast.Constant)
        self.assertTrue(all(isinstance(node, allowed) for node in ast.walk(tree)))
        return eval(
            compile(tree, "mobile input condition", "eval"), {"__builtins__": {}}
        )

    def run_commands(self, commands):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            (work / "dev").mkdir()
            (work / "bin").mkdir()
            shell = work / "dev/nix-shell"
            shell.write_text('#!/usr/bin/env bash\nexec bash -euc "$1"\n')
            shell.chmod(0o755)
            just = work / "bin/just"
            just.write_text(
                '#!/usr/bin/env bash\nprintf "%s\\n" "$*" >> "$RECEIPT"\nif [[ "$1 $2" == "backend ci" ]]; then shift 2; exec "$@"; fi\n'
            )
            just.chmod(0o755)
            receipt = work / "receipt"
            env = dict(
                os.environ,
                PATH=str(work / "bin") + ":" + os.environ["PATH"],
                RECEIPT=str(receipt),
            )
            env.pop("BASH_ENV", None)
            for command in commands:
                actual = subprocess.run(
                    ["bash", "-euc", command],
                    cwd=work,
                    env=env,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(actual.returncode, 0, actual.stderr)
            return receipt.read_text().splitlines() if receipt.exists() else []

    def test_ios_actual_commands_split_units_examples_and_simulator(self):
        data = self.workflow("test-ios.yml")
        inputs = data.get("on", data.get(True))["workflow_call"]["inputs"]
        self.assertEqual(set(inputs), {"run-unit", "run-platform"})
        self.assertTrue(all(item["default"] is True for item in inputs.values()))
        job = data["jobs"]["tests"]
        self.assertEqual(job["if"], "inputs.run-unit || inputs.run-platform")
        steps = {
            step.get("name"): step
            for step in job["steps"]
            if step.get("name", "").startswith("Run iOS")
        }
        self.assertEqual(
            set(steps), {"Run iOS unit tests and examples", "Run iOS simulator tests"}
        )
        for unit, platform in itertools.product((False, True), repeat=2):
            flags = {"run-unit": unit, "run-platform": platform}
            commands = [
                step["run"] for step in steps.values() if self.active(step["if"], flags)
            ]
            receipt = self.run_commands(commands)
            owners = [line for line in receipt if line.startswith("ios ")]
            expected = (
                ["ios test skip-seams", "ios check-examples"] if unit else []
            ) + (["ios test-simulator"] if platform else [])
            self.assertEqual(owners, expected)
            self.assertEqual(self.active(job["if"], flags), unit or platform)

    def test_android_actual_commands_keep_units_consumers_and_both_platform_jobs(self):
        data = self.workflow("test-android.yml")
        inputs = data.get("on", data.get(True))["workflow_call"]["inputs"]
        self.assertEqual(set(inputs), {"run-unit", "run-consumers", "run-platform"})
        self.assertTrue(all(item["default"] is True for item in inputs.values()))
        jobs = data["jobs"]
        self.assertEqual(jobs["min-sdk-smoke"]["timeout-minutes"], 45)
        self.assertEqual(jobs["integration-tests"]["timeout-minutes"], 45)
        integration = next(
            step
            for step in jobs["integration-tests"]["steps"]
            if step.get("name") == "Run integration tests"
        )
        self.assertEqual(integration["timeout-minutes"], 30)
        for unit, consumers, platform in itertools.product((False, True), repeat=3):
            flags = {
                "run-unit": unit,
                "run-consumers": consumers,
                "run-platform": platform,
            }
            active = {
                name
                for name, job in jobs.items()
                if name != "results" and self.active(job["if"], flags)
            }
            self.assertEqual(
                active,
                ({"unit-tests"} if unit or consumers else set())
                | ({"min-sdk-smoke", "integration-tests"} if platform else set()),
            )
            commands = [
                step["run"]
                for name, job in jobs.items()
                if name in active
                for step in job["steps"]
                if step.get("name")
                in (
                    "Run unit tests",
                    "Check Kotlin consumers",
                    "Run the minimum SDK smoke",
                    "Run integration tests",
                )
                and ("if" not in step or self.active(step["if"], flags))
            ]
            receipt = self.run_commands(commands)
            expected = (
                (["android test-min-sdk"] if platform else [])
                + (["android test"] if unit else [])
                + (["android check-consumers"] if consumers else [])
                + (["android test-integration"] if platform else [])
            )
            self.assertEqual(receipt, expected)
        self.assertEqual(
            jobs["unit-tests"]["if"], "inputs.run-unit || inputs.run-consumers"
        )
        self.assertEqual(jobs["min-sdk-smoke"]["if"], "inputs.run-platform")
        self.assertEqual(jobs["integration-tests"]["if"], "inputs.run-platform")

    def test_actual_android_gate_requires_each_selected_child_status(self):
        job = self.workflow("test-android.yml")["jobs"]["results"]
        self.assertEqual(job["if"], "always()")
        self.assertEqual(
            set(job["needs"]), {"unit-tests", "min-sdk-smoke", "integration-tests"}
        )
        step = next(step for step in job["steps"] if "run" in step)
        for flags in (
            (False, False, False),
            (True, False, False),
            (False, True, False),
            (False, False, True),
            (True, True, True),
        ):
            active = ({"unit-tests"} if flags[0] or flags[1] else set()) | (
                {"min-sdk-smoke", "integration-tests"} if flags[2] else set()
            )
            baseline = {
                name: {"result": "success" if name in active else "skipped"}
                for name in job["needs"]
            }
            for child in active:
                for status in ("success", "failure", "cancelled", "skipped", None):
                    needs = dict(baseline, **{child: {"result": status}})
                    env = dict(
                        os.environ,
                        RUN_UNIT=str(flags[0]).lower(),
                        RUN_CONSUMERS=str(flags[1]).lower(),
                        RUN_PLATFORM=str(flags[2]).lower(),
                        CI_NEEDS=json.dumps(needs),
                    )
                    actual = subprocess.run(
                        ["bash", "-euc", step["run"]],
                        cwd=ROOT,
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(
                        actual.returncode == 0, status == "success", actual.stderr
                    )

    def test_top_mobile_routes_pass_each_flag_and_gate_the_same_result(self):
        ci = self.workflow("ci.yml")["jobs"]
        for name, mapping in (
            ("test-ios", {"run-unit": "test_ios", "run-platform": "test_ios_platform"}),
            (
                "test-android",
                {
                    "run-unit": "test_android",
                    "run-consumers": "test_android_consumers",
                    "run-platform": "test_android_platform",
                },
            ),
        ):
            route = ci[name]
            self.assertEqual(route["needs"], "detect-changes")
            self.assertEqual(set(route["with"]), set(mapping))
            for values in itertools.product((False, True), repeat=len(mapping)):
                selections = dict(zip(mapping.values(), values))
                expression = route["if"]
                for flag, value in sorted(
                    selections.items(), key=lambda item: -len(item[0])
                ):
                    expression = expression.replace(
                        "fromJSON(needs.detect-changes.outputs.selection).checks."
                        + flag,
                        repr(value),
                    )
                self.assertEqual(self.active(expression, {}), any(values))
            for argument, flag in mapping.items():
                self.assertIn("checks." + flag, route["if"])
                self.assertEqual(
                    route["with"][argument],
                    "${{ fromJSON(needs.detect-changes.outputs.selection).checks."
                    + flag
                    + " }}",
                )
        steps = self.workflow("test-sdk-staging.yml")["jobs"]["android-stage"]["steps"]
        commands = [step["run"] for step in steps if "run" in step]
        for command in (
            "just sdk build kotlin --profile release",
            "just sdk render kotlin --profile release",
            "just sdk mobile-build android",
            "just sdk mobile-stage android",
        ):
            self.assertIn("dev/nix-shell '" + command + "'", commands)


if __name__ == "__main__":
    unittest.main()
