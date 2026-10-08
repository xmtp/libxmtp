#!/usr/bin/env python3
"""Check retained selection boundaries and the actual workflow result gates."""

from functools import cache
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]


def workflow(name):
    value = yaml.safe_load((ROOT / ".github/workflows" / name).read_text())
    if True in value:
        value["on"] = value.pop(True)
    return value


def passed(
    expression,
    checks=None,
    results=None,
    inputs=None,
    event="pull_request",
    plan=None,
    fork=False,
    draft=None,
    cancelled=False,
    dependency=False,
    matrix_check="licenses",
    raw=False,
    path_outputs=None,
    scope=None,
    outcome="success",
    changed_files=0,
):
    text = expression.removeprefix("${{").removesuffix("}}").strip()
    text = re.sub(
        r"steps.paths.outputs.([a-z_]+_count)\s*(>=|<)\s*(3000|github.event.pull_request.changed_files)",
        lambda m: (
            "int(path_outputs.get("
            + repr(m[1])
            + ", '0') or '0') "
            + m[2]
            + " "
            + ("3000" if m[3] == "3000" else "changed_files")
        ),
        text,
    )
    text = re.sub(
        r"steps.paths.outputs.([a-z_]+)",
        lambda m: "path_outputs.get(" + repr(m[1]) + ", '')",
        text,
    )
    text = re.sub(
        r"steps.scope.outputs.([a-z_]+)",
        lambda m: "scope.get(" + repr(m[1]) + ", '')",
        text,
    )
    text = text.replace("steps.paths.outcome", "outcome")
    text = text.replace("github.event.pull_request.changed_files", "changed_files")
    text = re.sub(
        r"fromJSON\(needs.detect-changes.outputs.selection\).checks.([a-z_]+)",
        lambda m: "checks.get(" + repr(m[1]) + ")",
        text,
    )
    text = re.sub(
        r"contains\(fromJSON\(needs.detect-changes.outputs.(lint_jobs|test_jobs)\), '([^']+)'\)",
        lambda m: repr(m[2]) + " in plan.get(" + repr(m[1]) + ", [])",
        text,
    )
    text = text.replace("needs[inputs.suite].result", "selected_result")
    text = re.sub(
        r"needs.([a-z_-]+).result", lambda m: "results.get(" + repr(m[1]) + ")", text
    )
    text = re.sub(
        r"inputs.([a-z_-]+)", lambda m: "inputs.get(" + repr(m[1]) + ")", text
    )
    text = text.replace("github.event_name", "event").replace("github.ref", "ref")
    text = text.replace("github.event.pull_request.head.repo.full_name", "head_repo")
    text = text.replace("github.event.pull_request.draft", "draft")
    text = text.replace("github.repository", "repository")
    text = text.replace("needs.draft-policy.outputs.run-deny", "run_deny")
    text = text.replace("matrix.checks", "matrix_check")
    text = text.replace("cancelled()", "is_cancelled")
    text = text.replace("&&", " and ").replace("||", " or ")
    text = re.sub(r"!(?!=)", " not ", text)
    value = eval(
        text,
        {"__builtins__": {}},
        {
            "checks": checks or {},
            "results": results or {},
            "inputs": inputs or {},
            "event": event,
            "ref": "refs/heads/self-hosted",
            "plan": plan or {},
            "head_repo": "fork/libxmtp" if fork else "xmtp/libxmtp",
            "repository": "xmtp/libxmtp",
            "selected_result": (results or {}).get((inputs or {}).get("suite")),
            "draft": draft,
            "is_cancelled": cancelled,
            "run_deny": "true" if dependency else "false",
            "matrix_check": matrix_check,
            "toJSON": json.dumps,
            "format": lambda pattern, value: pattern.format(value),
            "path_outputs": path_outputs or {},
            "scope": scope or {},
            "outcome": outcome,
            "changed_files": changed_files or 0,
            "int": int,
            "true": True,
            "false": False,
        },
    )
    return value if raw else bool(value)


def action_outputs(
    filters, rows, api_status=200, git_repo=None, before=None, list_files="none"
):
    """Run the pinned compiled action against a local PR-files endpoint."""

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            body = json.dumps(
                rows if api_status == 200 else {"message": "fixture failure"}
            ).encode()
            self.send_response(api_status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *args):
            pass

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        event = root / "event.json"
        payload = {"pull_request": {"number": 1}}
        if git_repo:
            payload = {"before": before, "repository": {"default_branch": "main"}}
        event.write_text(json.dumps(payload))
        output = root / "output"
        output.touch()
        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            env = {
                k: v
                for k, v in os.environ.items()
                if not k.startswith(("GITHUB_", "INPUT_", "ACTIONS_"))
            }
            env.update(
                GITHUB_EVENT_NAME="pull_request",
                GITHUB_EVENT_PATH=str(event),
                GITHUB_REPOSITORY="fixture/repo",
                GITHUB_WORKSPACE=str(root),
                GITHUB_API_URL=f"http://127.0.0.1:{server.server_port}",
                GITHUB_OUTPUT=str(output),
                INPUT_FILTERS=filters,
                INPUT_TOKEN="fixture",
            )
            env["INPUT_PREDICATE-QUANTIFIER"] = "some-with-excludes"
            env["INPUT_LIST-FILES"] = list_files
            if git_repo:
                env.update(
                    GITHUB_EVENT_NAME="push",
                    GITHUB_WORKSPACE=str(git_repo),
                    GITHUB_REF="refs/heads/main",
                    INPUT_BASE="refs/heads/main",
                    GIT_TRACE="1",
                )
                env["GITHUB_SHA"] = subprocess.check_output(
                    ["git", "-C", str(git_repo), "rev-parse", "HEAD"], text=True
                ).strip()
            result = subprocess.run(
                ["node", os.environ["PATHS_FILTER_ACTION"]],
                cwd=git_repo or root,
                env=env,
                capture_output=True,
                text=True,
                timeout=30,
            )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
        values = {}
        lines = iter(output.read_text().splitlines())
        for line in lines:
            if "<<" in line:
                key, delimiter = line.split("<<", 1)
                contents = []
                for value in lines:
                    if value == delimiter:
                        break
                    contents.append(value)
                values[key] = "\n".join(contents)
            elif "=" in line:
                key, value = line.split("=", 1)
                values[key] = value
        if git_repo:
            values["_git_trace"] = result.stderr + result.stdout
        return result.returncode, values


@cache
def matched_paths(filters, paths):
    result, values = action_outputs(
        filters, [{"filename": p, "status": "modified"} for p in paths]
    )
    assert result == 0, values
    return values


class WorkflowSelection:
    SOURCE_SUITES = tuple(
        json.loads(
            workflow("lint.yml")["on"]["workflow_call"]["inputs"]["suites"]["default"]
        )
    )
    TEST_SUITES = tuple(
        json.loads(
            workflow("test.yml")["on"]["workflow_call"]["inputs"]["suites"]["default"]
        )
    )

    @property
    def CHECKS(self):
        step = next(
            s
            for s in workflow("ci.yml")["jobs"]["detect-changes"]["steps"]
            if s.get("id") == "select"
        )
        return tuple(
            k.removeprefix("CHECK_").lower()
            for k in step["env"]
            if k.startswith("CHECK_")
        ) + ("test_bindings",)

    def select(
        self,
        paths,
        event="pull_request",
        fork=False,
        draft=None,
        outputs=None,
        outcome=None,
        changed_files=None,
    ):
        steps = workflow("ci.yml")["jobs"]["detect-changes"]["steps"]
        path_step = next(s for s in steps if s.get("id") == "paths")
        if outputs is None:
            outputs = (
                {}
                if paths is None
                else matched_paths(path_step["with"]["filters"], tuple(paths))
            )
        outcome = outcome or ("failure" if paths is None else "success")
        context = dict(
            event=event,
            fork=fork,
            draft=draft,
            path_outputs=outputs,
            outcome=outcome,
            changed_files=len(paths or []) if changed_files is None else changed_files,
        )
        scope_step = next(s for s in steps if s.get("id") == "scope")
        scope = {
            name.lower(): "true" if passed(expression, **context) else "false"
            for name, expression in scope_step["env"].items()
        }
        select_step = next(s for s in steps if s.get("id") == "select")
        env = {}
        for name, expression in select_step["env"].items():
            value = passed(expression, scope=scope, raw=True, **context)
            env[name] = (
                value if isinstance(value, str) else "true" if value else "false"
            )
        return json.loads(assembled_plan(select_step["run"], tuple(env.items())))


@cache
def assembled_plan(command, values):
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / "outputs"
        result = subprocess.run(
            ["bash", "-euc", command],
            env={
                **os.environ,
                **dict(values),
                "RUNNER_TEMP": directory,
                "GITHUB_OUTPUT": str(output),
            },
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, result.stderr
        return (Path(directory) / "ci-selection.json").read_text()


selection = WorkflowSelection()


class SelectionTests(unittest.TestCase):
    def test_pure_rust_pr_keeps_rust_and_omits_hosts(self):
        checks = selection.select(["crates/xmtp_mls/src/lib.rs"])["checks"]
        expected = {
            "lint_workspace",
            "check_rust",
            "test_workspace",
            "test_wasm",
            "test_backend",
            "test_native_backend",
            "test_validation",
            "test_keepalive",
            "test_xdbg",
            "check_sdk_unit",
            "docs_rust",
            "docs_quality",
        }
        self.assertEqual({name for name in selection.CHECKS if checks[name]}, expected)

    def test_core_push_keeps_units_and_consumers_without_platform_packages(self):
        checks = selection.select(["crates/xmtp_mls/src/lib.rs"], "push")["checks"]
        for name in (
            "test_ios",
            "test_swift_lifecycle",
            "test_android",
            "test_android_consumers",
            "test_node",
            "test_browser",
            "test_agent",
            "check_sdk",
        ):
            self.assertTrue(checks[name], name)
        for name in (
            "test_ios_platform",
            "test_android_platform",
            "test_sdk_staging",
            "docs_site",
        ):
            self.assertFalse(checks[name], name)

    def test_build_dependency_generator_service_and_unknown_inputs_run_all(self):
        for path in (
            "Cargo.lock",
            ".config/hakari.toml",
            "nix/new.nix",
            "apps/new/Cargo.toml",
            "apps/backend/src/main.rs",
            "apps/xmtp_sdk_bindgen/src/lib.rs",
            "crates/xmtp_sdk/src/lib.rs",
            "crates/new/src/lib.rs",
            ".github/workflows/test.yml",
            "dev/new-helper",
            "unknown",
        ):
            with self.subTest(path=path):
                self.assertTrue(
                    all(
                        selection.select([path])["checks"][name]
                        for name in selection.CHECKS
                    )
                )

    def test_docs_and_native_input_boundaries(self):
        self.assertTrue(selection.select(["docs/guide.md"])["checks"]["docs_site"])
        self.assertFalse(
            selection.select(["sdks/ios/Tests/XmtpSdkTests/ReaderTeardownTests.swift"])[
                "checks"
            ]["docs_site"]
        )
        for path in (
            "sdks/ios/Sources/XmtpSdk/Client.swift",
            "sdks/android/library/src/main/Client.kt",
            "sdks/node/src/index.ts",
            "sdks/browser/src/index.ts",
        ):
            self.assertTrue(selection.select([path])["checks"]["docs_site"], path)
        c = selection.select(["sdks/android/library/src/test/ReaderTest.kt"])["checks"]
        self.assertTrue(c["test_android_consumers"])
        self.assertFalse(c["test_android_platform"])
        c = selection.select(
            ["sdks/android/library/src/androidTest/AndroidPackageTest.kt"]
        )["checks"]
        self.assertTrue(c["test_android_platform"])

    def test_manual_missing_diff_and_fork_policy(self):
        self.assertTrue(
            all(selection.select(None)["checks"][n] for n in selection.CHECKS)
        )
        self.assertTrue(
            all(
                selection.select([], "workflow_dispatch")["checks"][n]
                for n in selection.CHECKS
            )
        )
        c = selection.select(None, fork=True)["checks"]
        for name in (
            "test_native_backend",
            "test_ios",
            "test_ios_platform",
            "test_swift_lifecycle",
        ):
            self.assertFalse(c[name])
        self.assertTrue(c["test_android_platform"])

    def test_browser_platform_keeps_explicit_generated_type_checks(self):
        for event in ("pull_request", "push"):
            for filename in (
                "suite.worker.ts",
                "attachment-lifetime.chromium.ts",
                "storage.layout.chromium.ts",
            ):
                with self.subTest(event=event, filename=filename):
                    checks = selection.select(
                        ["sdks/browser/test/platform/" + filename], event
                    )["checks"]
                    self.assertTrue(checks["check_sdk"])
                    self.assertTrue(checks["test_browser_platform"])
                    self.assertFalse(checks["docs_site"])

    def test_draft_source_routes_and_full_modes(self):
        for paths in (
            ["crates/xmtp_mls/src/lib.rs"],
            ["sdks/node/src/index.ts"],
            ["Cargo.toml"],
            ["proto/foo.proto"],
            ["docs/guide.md"],
            ["unknown"],
            None,
        ):
            for fork in (False, True):
                ready = selection.select(paths, fork=fork)
                draft = selection.select(paths, fork=fork, draft=True)
                expected = {
                    name for name in selection.SOURCE_SUITES if ready["checks"][name]
                } | {"docs_quality"}
                self.assertEqual(
                    {name for name in selection.CHECKS if draft["checks"][name]},
                    expected,
                )
                self.assertEqual(draft["test_jobs"], ["detect-changes"])
                self.assertTrue(
                    set(draft["lint_jobs"])
                    <= {"detect-changes", "source-lint", "docs-quality"}
                )
                for unknown in (None, False, "true", 1):
                    self.assertEqual(
                        selection.select(paths, fork=fork, draft=unknown), ready
                    )
                for event in ("push", "workflow_dispatch"):
                    self.assertEqual(
                        selection.select(paths, event, draft=True),
                        selection.select(paths, event),
                    )

    def test_draft_transitions_and_distinct_names(self):
        events = {
            "opened",
            "synchronize",
            "reopened",
            "ready_for_review",
            "converted_to_draft",
        }
        for filename in ("ci.yml", "cargo-deny-checker.yml"):
            value = workflow(filename)
            self.assertEqual(set(value["on"]["pull_request"]["types"]), events)
            self.assertIs(value["concurrency"]["cancel-in-progress"], True)
        for action, draft in (
            ("converted_to_draft", True),
            ("ready_for_review", False),
        ):
            for phase, ready_name, draft_name in (
                ("lint", "Lint", "Draft lint"),
                ("test", "Test", "Draft checks"),
            ):
                self.assertEqual(
                    passed(
                        workflow("ci.yml")["jobs"][phase]["name"], draft=draft, raw=True
                    ),
                    draft_name if draft else ready_name,
                    action,
                )
        for unknown in (None, "true", 1):
            self.assertEqual(
                passed(
                    workflow("ci.yml")["jobs"]["lint"]["name"], draft=unknown, raw=True
                ),
                "Lint",
            )
        self.assertEqual(
            passed(
                workflow("ci.yml")["jobs"]["test"]["name"],
                draft=True,
                event="push",
                raw=True,
            ),
            "Test",
        )

    def test_public_source_rename_keeps_both_paths(self):
        step = next(
            s
            for s in workflow("ci.yml")["jobs"]["detect-changes"]["steps"]
            if s.get("id") == "paths"
        )
        rows = [
            {
                "filename": "sdks/browser/test/codec.ts",
                "previous_filename": "sdks/browser/src/codec.ts",
                "status": "renamed",
            }
        ]
        code, values = action_outputs(step["with"]["filters"], rows)
        self.assertEqual(code, 0)
        self.assertEqual(values["all_files_count"], "2")
        for event in ("pull_request", "push"):
            plan = selection.select([], event, outputs=values, changed_files=1)
            self.assertTrue(plan["checks"]["docs_site"])
            self.assertTrue(plan["checks"]["test_browser"])

    def test_bundled_action_fetches_only_missing_push_sha(self):
        step = next(
            s
            for s in workflow("ci.yml")["jobs"]["detect-changes"]["steps"]
            if s.get("id") == "paths"
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()

            def git(*args, cwd=source):
                return subprocess.check_output(
                    ["git", *args], cwd=cwd, text=True
                ).strip()

            git("init", "-qb", "main")
            git("config", "user.name", "Fixture")
            git("config", "user.email", "fixture@example.invalid")
            path = source / "crates/xmtp_mls/src/lib.rs"
            path.parent.mkdir(parents=True)
            for count in range(5):
                path.write_text(str(count))
                git("add", ".")
                git("commit", "-qm", "source update")
                if count == 0:
                    before = git("rev-parse", "HEAD")
            checkout = root / "checkout"
            git("clone", "--depth=2", source.as_uri(), str(checkout), cwd=root)
            absent = subprocess.run(
                ["git", "cat-file", "-e", before], cwd=checkout, capture_output=True
            )
            self.assertNotEqual(absent.returncode, 0)
            code, values = action_outputs(
                step["with"]["filters"], [], git_repo=checkout, before=before
            )
            self.assertEqual(code, 0, values)
            self.assertEqual(values["rust"], "true")
            trace = values["_git_trace"]
            self.assertIn("fetch --depth=1 --no-tags origin " + before, trace)
            self.assertIn("--no-renames", trace)
            code, second = action_outputs(
                step["with"]["filters"], [], git_repo=checkout, before=before
            )
            self.assertEqual(code, 0)
            self.assertNotIn(
                "fetch --depth=1 --no-tags origin " + before, second["_git_trace"]
            )

    def test_action_errors_partial_and_capped_lists_force_full(self):
        step = next(
            s
            for s in workflow("ci.yml")["jobs"]["detect-changes"]["steps"]
            if s.get("id") == "paths"
        )
        code, outputs = action_outputs(step["with"]["filters"], [], api_status=401)
        self.assertNotEqual(code, 0)
        self.assertTrue(
            all(
                selection.select([], outputs=outputs, outcome="failure")["checks"][name]
                for name in selection.CHECKS
            )
        )
        values = matched_paths(step["with"]["filters"], ("crates/xmtp_mls/src/lib.rs",))
        for counts in (3000, 2):
            self.assertTrue(
                all(
                    selection.select([], outputs=values, changed_files=counts)[
                        "checks"
                    ][name]
                    for name in selection.CHECKS
                )
            )
        capped = {**values, "all_files_count": "3000", "known_files_count": "3000"}
        self.assertTrue(selection.select([], outputs=capped)["checks"]["check_sdk"])
        draft = selection.select([], outputs=capped, draft=True)
        self.assertEqual(draft["test_jobs"], ["detect-changes"])

    def test_cargo_deny_draft_scope_and_cancelled_policy(self):
        value = workflow("cargo-deny-checker.yml")
        policy = value["jobs"]["draft-policy"]
        paths = next(s for s in policy["steps"] if s.get("id") == "paths")
        decision = next(s for s in policy["steps"] if s.get("id") == "select")["env"][
            "RUN_DENY"
        ]
        for filename, expected in (
            ("Cargo.lock", True),
            ("Cargo.toml", True),
            ("crates/xmtp_mls/Cargo.toml", True),
            ("deny.toml", True),
            (".github/workflows/cargo-deny-checker.yml", True),
            ("crates/xmtp_mls/src/lib.rs", False),
            ("docs/guide.md", False),
        ):
            code, outputs = action_outputs(
                paths["with"]["filters"], [{"filename": filename, "status": "modified"}]
            )
            self.assertEqual(code, 0)
            self.assertEqual(
                passed(decision, path_outputs=outputs, changed_files=1),
                expected,
                filename,
            )
        self.assertTrue(passed(decision, path_outputs={}, outcome="failure"))
        self.assertTrue(passed(decision, path_outputs={"all_files_count": "3000"}))
        self.assertEqual(
            policy["permissions"], {"contents": "read", "pull-requests": "read"}
        )
        self.assertNotIn("continue-on-error", policy)
        matrix = value["jobs"]["cargo-deny"]
        self.assertEqual(
            matrix["strategy"]["matrix"]["checks"],
            ["advisories", "bans", "licenses", "sources"],
        )
        self.assertEqual(
            matrix["continue-on-error"], "${{ matrix.checks == 'advisories' }}"
        )
        for event, draft, status, dependency, expected in (
            ("pull_request", False, "skipped", False, True),
            ("push", True, "skipped", False, True),
            ("workflow_dispatch", True, "skipped", False, True),
            ("pull_request", True, "success", True, True),
            ("pull_request", True, "success", False, False),
            ("pull_request", True, "failure", True, False),
            ("pull_request", True, "skipped", True, False),
        ):
            self.assertEqual(
                passed(
                    matrix["if"],
                    event=event,
                    draft=draft,
                    results={"draft-policy": status},
                    dependency=dependency,
                ),
                expected,
            )
            self.assertFalse(
                passed(
                    matrix["if"],
                    event=event,
                    draft=draft,
                    results={"draft-policy": status},
                    dependency=dependency,
                    cancelled=True,
                )
            )
        self.assertEqual(passed(matrix["name"], raw=True), "cargo-deny (licenses)")
        self.assertEqual(
            passed(matrix["name"], draft=True, raw=True), "Draft cargo-deny (licenses)"
        )

    def test_windows_dependencies_fetch_exact_sha_with_one_commit(self):
        for filename, job, target in (
            (
                "build-sdk-node-platforms.yml",
                "windows",
                "target/sdk-node-runtime-source",
            ),
            ("test-sdk.yml", "windows-load", "ubrn"),
        ):
            commands = "\n".join(
                s.get("run", "") for s in workflow(filename)["jobs"][job]["steps"]
            )
            self.assertNotIn("git clone", commands)
            self.assertIn(
                "git -C " + target + ' fetch --no-tags --depth=1 origin "$fork_rev"',
                commands,
            )
            self.assertIn(
                "git -C " + target + ' checkout --detach "$fork_rev"', commands
            )
            self.assertIn(
                "https://github.com/neekolas/uniffi-bindgen-react-native.git", commands
            )

    def test_static_routers_and_matrix_results_reject_skipped_missing_and_failure(self):
        for filename, names in (
            ("test.yml", selection.TEST_SUITES),
            ("lint.yml", selection.SOURCE_SUITES),
        ):
            top = workflow(filename)
            caller = top["on"]["workflow_call"]["inputs"]
            self.assertEqual(json.loads(caller["suites"]["default"]), list(names))
            matrix = top["jobs"]["suites"]
            self.assertIs(matrix["strategy"]["fail-fast"], True)
            self.assertEqual(
                matrix["strategy"]["matrix"]["suite"], "${{ fromJSON(inputs.suites) }}"
            )
            router = workflow(matrix["uses"].split("/")[-1])
            self.assertEqual(set(router["jobs"]) - {"result"}, set(names))
            self.assertEqual(set(router["jobs"]["result"]["needs"]), set(names))
            result = router["jobs"]["result"]["steps"][0]["env"]["PASSED"]
            for name in names:
                self.assertIn(name, router["jobs"])
                self.assertTrue(
                    passed(result, results={name: "success"}, inputs={"suite": name})
                )
                for status in ("failure", "cancelled", "skipped", None):
                    self.assertFalse(
                        passed(result, results={name: status}, inputs={"suite": name})
                    )
            self.assertFalse(passed(result, inputs={"suite": "unknown"}))
            result = top["jobs"]["results"]["steps"][0]["env"]["PASSED"]
            for status in ("failure", "cancelled", "skipped", None):
                self.assertFalse(passed(result, results={"suites": status}))

    def test_required_and_mobile_gates_use_selected_success(self):
        top = workflow("ci.yml")
        plan = selection.select(None)
        for name in ("lint", "test"):
            gate = top["jobs"][name]
            step = gate["steps"][0]
            key = name + "_jobs"
            required = plan[key]
            self.assertEqual(
                step["env"]["REQUIRED_JOBS"],
                "${{ needs.detect-changes.outputs." + key + " }}",
            )
            self.assertEqual(step["env"]["JOB_RESULTS"], "${{ toJSON(needs) }}")

            def run_gate(results, required_json=None):
                return (
                    subprocess.run(
                        ["bash", "-euc", step["run"]],
                        env={
                            **os.environ,
                            "REQUIRED_JOBS": json.dumps(required)
                            if required_json is None
                            else required_json,
                            "JOB_RESULTS": json.dumps(
                                {
                                    job: {"result": status}
                                    for job, status in results.items()
                                }
                            ),
                        },
                        capture_output=True,
                        text=True,
                    ).returncode
                    == 0
                )

            results = {
                job: "success" if job in required else "skipped"
                for job in gate["needs"]
            }
            self.assertTrue(run_gate(results))
            for job in required:
                for status in ("failure", "cancelled", "skipped", None):
                    self.assertFalse(
                        run_gate({**results, job: status}), (name, job, status)
                    )
                missing = dict(results)
                del missing[job]
                self.assertFalse(run_gate(missing), (name, job, "missing"))
            self.assertFalse(run_gate(results, ""))
            self.assertFalse(run_gate(results, '["unknown"]'))

            # Rust-only changes use the standalone reference; the full site owns it otherwise.
            rust = selection.select(["crates/xmtp_mls/src/lib.rs"])
            required = rust[key]
            results = {
                job: "success" if job in required else "skipped"
                for job in gate["needs"]
            }
            self.assertTrue(run_gate(results))
            if name == "test":
                self.assertIn("docs-rust-reference", required)
                self.assertNotIn("docs", required)
                self.assertIn("docs", plan[key])
                self.assertNotIn("docs-rust-reference", plan[key])

    def test_selected_lists_agree_with_caller_scheduling(self):
        top = workflow("ci.yml")
        for paths, event, fork in (
            (None, "workflow_dispatch", False),
            (None, "pull_request", True),
            ([], "pull_request", False),
            (["crates/xmtp_mls/src/lib.rs"], "pull_request", False),
            (["crates/xmtp_mls/src/lib.rs"], "push", False),
            (["sdks/ios/Sources/XmtpSdk/Client.swift"], "pull_request", False),
            (
                ["sdks/android/library/src/androidTest/AndroidPackageTest.kt"],
                "pull_request",
                False,
            ),
            (["docs/guide.md"], "pull_request", False),
        ):
            plan = selection.select(paths, event, fork)
            for phase in ("lint", "test"):
                key = phase + "_jobs"
                required = plan[key]
                self.assertEqual(required.count("detect-changes"), 1)
                self.assertTrue(set(required).issubset(top["jobs"][phase]["needs"]))
                self.assertEqual(
                    top["jobs"]["detect-changes"]["outputs"][key],
                    "${{ toJSON(fromJSON(steps.select.outputs.selection)."
                    + key
                    + ") }}",
                )
                for job in top["jobs"][phase]["needs"]:
                    if job == "detect-changes":
                        continue
                    expression = top["jobs"][job]["if"]
                    self.assertIn("outputs." + key, expression)
                    self.assertEqual(
                        passed(expression, plan=plan, event=event, fork=fork),
                        job in required,
                        (paths, event, fork, job),
                    )

    def test_mobile_gates_use_selected_success(self):
        expression = workflow("test-android.yml")["jobs"]["results"]["steps"][0]["env"][
            "PASSED"
        ]
        inputs = {"run-unit": True, "run-consumers": True, "run-platform": True}
        defaults = workflow("test-android.yml")["on"]["workflow_call"]["inputs"]
        for name in inputs:
            self.assertIs(defaults[name]["default"], True)
        results = dict.fromkeys(
            ("unit-tests", "min-sdk-smoke", "integration-tests"), "success"
        )
        self.assertTrue(passed(expression, results=results, inputs=inputs))
        for job in results:
            for status in ("failure", "cancelled", "skipped", None):
                self.assertFalse(
                    passed(expression, results={**results, job: status}, inputs=inputs)
                )

    def test_command_jobs_select_small_shells(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            value = workflow(path.name)
            for name, job in value.get("jobs", {}).items():
                for step in job.get("steps", []):
                    command = step.get("run", "")
                    if not re.search(r"(?:dev/nix-shell|\bjust\s)", command):
                        continue
                    if job.get("runs-on") == "windows-latest":
                        continue
                    shell = {
                        **value.get("env", {}),
                        **job.get("env", {}),
                        **step.get("env", {}),
                    }.get("NIX_DEVSHELL")
                    explicit = re.search(
                        r"(?:--shell\s+|NIX_DEVSHELL=)(rust|js-node|js|ios|android|wasm|docs)",
                        command,
                    )
                    self.assertTrue(shell or explicit, (path.name, name, command))
                    self.assertNotEqual(shell, "default", (path.name, name))

    def test_recovery_and_windows_stay_manual(self):
        self.assertEqual(
            set(workflow("manual-sdk-recovery.yml")["on"]), {"workflow_dispatch"}
        )
        self.assertNotIn("recovery", workflow("test-node-sdk.yml")["jobs"])
        self.assertEqual(
            workflow("test-sdk.yml")["jobs"]["windows-load"]["if"],
            "github.event_name == 'workflow_dispatch'",
        )

    def test_backend_publisher_keeps_release_paths_without_pr_duplicates(self):
        publisher = workflow("push-backend.yml")
        self.assertEqual(set(publisher["on"]), {"push", "workflow_call"})
        self.assertEqual(
            publisher["on"]["push"],
            {"branches": ["main", "self-hosted"], "tags": ["**"]},
        )
        matrix = publisher["jobs"]["publish"]["strategy"]["matrix"]["include"]
        self.assertEqual({row["arch"] for row in matrix}, {"amd64", "arm64"})
        release = workflow("release-backend.yml")["jobs"]["build"]
        self.assertEqual(release["uses"], "./.github/workflows/push-backend.yml")
        self.assertIs(release["with"]["release-build"], True)
        manifest = publisher["jobs"]["manifest"]
        self.assertIn("publish", manifest["needs"])
        command = next(
            step["run"]
            for step in manifest["steps"]
            if step.get("name") == "Publish commit manifest"
        )
        self.assertIn('"$AMD64_IMAGE" "$ARM64_IMAGE"', command.splitlines()[0])
        expression = workflow("test-backend.yml")["jobs"]["backend-image"]["strategy"][
            "matrix"
        ]["arch"]
        expression = expression.removeprefix("${{ fromJSON(").removesuffix(") }}")
        expression = expression.replace("github.event_name", "event")
        expression = expression.replace("&&", " and ").replace("||", " or ")
        for event, expected in (
            ("pull_request", ["amd64"]),
            ("push", ["amd64"]),
            ("workflow_dispatch", ["amd64", "arm64"]),
        ):
            arches = eval(expression, {"__builtins__": {}}, {"event": event})
            self.assertEqual(json.loads(arches), expected)


if __name__ == "__main__":
    unittest.main()
