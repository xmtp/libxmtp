#!/usr/bin/env python3
"""Check CI suite selection, result gates, and decision summaries."""

import copy
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest import mock

import yaml


ROOT = Path(__file__).resolve().parents[2]
LOADER = importlib.machinery.SourceFileLoader("ci_select", str(ROOT / "dev/ci-select"))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
SELECTOR = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(SELECTOR)


def event(draft=False, fork=False, labels=()):
    return {
        "repository": {"full_name": "xmtp/libxmtp"},
        "pull_request": {
            "number": 123,
            "draft": draft,
            "changed_files": 1,
            "labels": [{"name": label} for label in labels],
            "head": {
                "sha": "event-head",
                "repo": {
                    "full_name": "fork/libxmtp" if fork else "xmtp/libxmtp",
                },
            },
        },
    }


def select(
    files=("docs/guide.md",),
    name="pull_request",
    payload=None,
    outcome="success",
    total=1,
    error=None,
    deleted=(),
    token=True,
):
    return SELECTOR.select_suites(
        None if files is None else list(files),
        name,
        event() if payload is None else payload,
        outcome,
        total,
        error,
        deleted,
        token,
    )


def selected(result):
    """The blocking suites: the ones the Lint and Test gates wait for."""
    return set(result["suites"]["lint"] + result["suites"]["test"])


def advisory(result):
    return set(result["advisory"])


def eligible(kind, fork=False):
    return {
        n
        for n, s in SELECTOR.SUITES.items()
        if kind in s["run_on"] and not (fork and s["disable_on_forks"])
    }


class SelectionTests(unittest.TestCase):
    def test_docs_select_docs_suites_only(self):
        result = select()
        self.assertEqual(result["reasons"], [])
        self.assertEqual(result["suites"]["lint"], ["docs_quality"])
        self.assertEqual(result["suites"]["test"], ["docs_site"])

    def test_rust_closures_narrow_selection(self):
        client = selected(select(["crates/xmtp_mls/src/client.rs"]))
        self.assertIn("test_workspace", client)
        self.assertTrue(
            client.isdisjoint(
                {"test_backend", "test_validation", "test_keepalive", "test_node"}
            )
        )
        backend_doc = selected(select(["docs/backend-observability.md"]))
        self.assertIn("test_backend", backend_doc)
        proto = selected(select(["crates/xmtp_proto/src/lib.rs"]))
        self.assertIn("test_validation", proto)

    def test_backend_change_runs_backend_and_client_suites_only(self):
        # Backend source/config changes select client suites without platform
        # wrappers. Its Nix package also runs the example apps, as advisory.
        clients = {"test_workspace", "test_wasm", "test_node", "check_sdk_unit"}
        platforms = {
            "test_android",
            "test_example_android",
            "test_example_ios",
            "test_ios",
            "test_browser",
            "test_agent",
            "test_sdk_platform",
        }
        for path in (
            "apps/backend/src/main.rs",
            "nix/package/backend.nix",
            "dev/docker/up",
            "dev/docker/toxiproxy/config.json",
            "dev/backend/local-s3.toml",
        ):
            with self.subTest(path=path):
                result = select([path])
                suites = selected(result)
                self.assertTrue(clients | {"test_backend", "test_xdbg"} <= suites)
                self.assertTrue(suites.isdisjoint(platforms))
                self.assertEqual(
                    advisory(result),
                    {"test_example_android", "test_example_ios"}
                    if path == "nix/package/backend.nix"
                    else set(),
                )

    def test_android_paths_select_the_app_and_sdk_suites(self):
        # path: (suites it must select, suites it must not select)
        app_only = {"test_android", "check_sdk", "check_sdk_unit"}
        table = {
            "apps/example-android/app/src/main/java/Screen.kt": (
                {"test_example_android"},
                app_only,
            ),
            "apps/example-android/fixtures/metadata_backend.py": (
                {"test_example_android"},
                app_only,
            ),
            "nix/package/example-android-apk.nix": (
                {"test_example_android"},
                app_only,
            ),
            # Decision D1: the Kotlin SDK runs the app as advisory only.
            "sdks/android/library/src/main/java/Client.kt": (
                {"test_android"},
                {"test_example_android"},
            ),
            "sdks/android/gradle/verification-metadata.xml": (
                {"test_android"},
                {
                    "check_rust",
                    "test_workspace",
                    "test_node",
                    "test_ios",
                    "test_example_android",
                },
            ),
            "crates/xmtp_sdk/src/client.rs": (
                {"test_android"},
                {"test_example_android"},
            ),
        }
        for path, (included, excluded) in table.items():
            with self.subTest(path=path):
                result = select([path])
                self.assertEqual(result["unknown_paths"], [])
                self.assertLessEqual(included, selected(result))
                self.assertTrue(excluded.isdisjoint(selected(result)))
        result = select(["sdks/android/library/src/main/java/Client.kt"])
        self.assertEqual(advisory(result), {"test_example_android"})

    def test_examples_match_app_paths_or_platform_labels(self):
        ios, android = "test_example_ios", "test_example_android"
        examples = {ios, android}
        swift = "sdks/ios/Sources/Client.swift"
        kotlin = "sdks/android/library/src/main/java/Client.kt"
        # Decision D1: the app's own paths and its label block; the SDK the
        # app consumes runs it as advisory. (blocking, advisory)
        for paths, labels, blocking, advisory_only in [
            (["apps/example-ios/ExampleApp/View.swift"], [], {ios}, set()),
            (
                ["apps/example-android/app/src/main/java/Screen.kt"],
                [],
                {android},
                set(),
            ),
            (["apps/example-ios/README.md"], [], {ios}, set()),
            (["apps/example-android/README.md"], [], {android}, set()),
            (["apps/example-ios/AGENTS.md"], [], set(), set()),
            ([swift], [], set(), {ios}),
            ([kotlin], [], set(), {android}),
            ([swift, "apps/example-ios/ExampleApp/View.swift"], [], {ios}, set()),
            ([swift], ["iOS"], {ios}, set()),
            (["crates/xmtp_sdk/src/client.rs"], [], set(), set()),
            (["apps/backend/src/main.rs"], [], set(), set()),
            (["dev/backend/local.toml"], [], set(), set()),
            (["docs/guide.md"], ["iOS"], {ios}, set()),
            (["docs/guide.md"], ["android"], {android}, set()),
            (["docs/guide.md"], ["iOS", "android"], examples, set()),
            ([], ["iOS"], {ios}, set()),
            (["sdks/ios/AGENTS.md"], ["iOS"], {ios}, set()),
            (["docs/guide.md"], ["ios", "Android", "unrelated"], set(), set()),
            (["apps/example-ios/ExampleApp/View.swift"], ["android"], examples, set()),
            (
                ["apps/example-android/app/src/main/java/Screen.kt"],
                ["iOS"],
                examples,
                set(),
            ),
            ([kotlin], ["iOS"], {ios}, {android}),
        ]:
            with self.subTest(paths=paths, labels=labels):
                result = select(paths, payload=event(labels=labels), total=len(paths))
                self.assertEqual(result["reasons"], [])
                self.assertEqual(selected(result) & examples, blocking)
                self.assertEqual(advisory(result), advisory_only)
                self.assertEqual(
                    result["matched_labels"], sorted(set(labels) & {"iOS", "android"})
                )

    def test_example_nix_inputs_select_affected_apps(self):
        # The app packaging blocks. SDK, toolchain, and service inputs run the
        # app as advisory (decision D1).
        ios, android = "test_example_ios", "test_example_android"
        examples = {ios, android}
        for path, blocking, advisory_only in [
            ("nix/package/xmtp-sdk.nix", set(), examples),
            ("nix/package/xmtp-sdk-native.nix", set(), examples),
            ("nix/lib/sdk-sources.nix", set(), examples),
            ("nix/lib/sdk-provenance.nix", set(), examples),
            ("nix/sdk-platforms.json", set(), examples),
            ("nix/lib/android-env.nix", set(), examples),
            ("nix/lib/ios-env.nix", set(), examples),
            ("nix/package/backend.nix", set(), examples),
            ("nix/musl-docker.nix", set(), examples),
            ("nix/package/example-android-apk.nix", {android}, set()),
            ("nix/package/example-android-apk-deps.json", {android}, set()),
            ("nix/package/libffi-zig-cross.site", set(), {android}),
            ("nix/android-packages.nix", set(), {android}),
            ("nix/lib/android-emulator-platform.sh", set(), {android}),
            ("nix/lib/android-emulator-start.py", set(), {android}),
            ("nix/lib/android-sync-clock.sh", set(), {android}),
            ("nix/shells/android.nix", set(), {android}),
            ("nix/shells/android-ci.nix", set(), {android}),
            ("nix/ci-sets.nix", set(), {android}),
            ("nix/package/example-ios-run.nix", {ios}, set()),
            ("nix/package/ios.nix", set(), {ios}),
            ("nix/lib/packages/swiftformat.nix", set(), {ios}),
            ("nix/lib/packages/swiftlint.nix", set(), {ios}),
            ("nix/lib/packages/pnpm.nix", set(), {ios}),
            ("nix/lib/packages/wasm-bindgen-cli.nix", set(), {ios}),
            ("nix/services/default.nix", set(), {ios}),
            ("nix/services/stack.nix", set(), {ios}),
            ("nix/services/health-check.sh", set(), {ios}),
            ("nix/docs.nix", set(), set()),
            ("nix/js.nix", set(), set()),
            ("nix/package/node.nix", set(), set()),
            ("nix/package/xmtp-sdk-node-runtime.nix", set(), set()),
            ("nix/lib/packages/ubrn.nix", set(), set()),
            ("nix/package/backend-tests.nix", set(), set()),
            ("nix/fmt.nix", set(), set()),
        ]:
            with self.subTest(path=path):
                result = select([path])
                self.assertEqual(result["reasons"], [])
                self.assertEqual(selected(result) & examples, blocking)
                self.assertEqual(advisory(result), advisory_only)

    def test_label_matches_respect_drafts_and_forks(self):
        with mock.patch.dict(SELECTOR.SUITES["test_ios"], labels=["iOS"]):
            for draft, fork, expected in [
                (
                    False,
                    False,
                    {"test_ios", "test_example_ios", "test_example_android"},
                ),
                (False, True, {"test_example_ios", "test_example_android"}),
                (True, False, set()),
                (True, True, set()),
            ]:
                with self.subTest(draft=draft, fork=fork):
                    result = select(
                        payload=event(draft=draft, fork=fork, labels=["iOS", "android"])
                    )
                    self.assertEqual(
                        selected(result)
                        & {"test_ios", "test_example_ios", "test_example_android"},
                        expected,
                    )

    def test_full_runs_keep_example_suites(self):
        # A full run of a pull request runs the example apps as advisory
        # unless the app's own paths or label match. Merge and manual runs
        # block on them.
        examples = {"test_example_ios", "test_example_android"}
        for kwargs, tier in [
            ({"files": ["flake.nix"]}, "advisory"),
            ({"files": ["flake.nix", "apps/example-ios/App.swift"]}, "mixed"),
            ({"files": ["new-language/source.xyz"]}, "advisory"),
            ({"total": 3001}, "advisory"),
            ({"total": None}, "advisory"),
            ({"outcome": "failure"}, "advisory"),
            ({"files": None}, "advisory"),
            ({"name": "push"}, "blocking"),
            ({"name": "workflow_dispatch"}, "blocking"),
        ]:
            with self.subTest(kwargs=kwargs):
                result = select(**kwargs)
                self.assertTrue(result["reasons"])
                if tier == "blocking":
                    self.assertLessEqual(examples, selected(result))
                    self.assertEqual(advisory(result), set())
                elif tier == "advisory":
                    self.assertEqual(advisory(result), examples)
                    self.assertTrue(examples.isdisjoint(selected(result)))
                else:
                    self.assertIn("test_example_ios", selected(result))
                    self.assertEqual(advisory(result), {"test_example_android"})

    def test_shared_input_selects_every_suite_and_records_paths(self):
        result = select(["proto/mls/a.proto", "crates/xmtp_mls/src/client.rs"])
        self.assertEqual(result["reasons"], ["shared_input"])
        self.assertEqual(result["shared_paths"], ["proto/mls/a.proto"])
        tiered = {
            name
            for name, suite in SELECTOR.SUITES.items()
            if SELECTOR.has_advisory_tier(suite)
        }
        self.assertEqual(tiered, {"test_example_ios", "test_example_android"})
        self.assertEqual(
            selected(result), eligible("ready_pr_push") - {"docs_rust"} - tiered
        )
        self.assertEqual(advisory(result), tiered)

    def test_unknown_paths_select_every_suite_and_are_named(self):
        result = select(["new-language/source.xyz"])
        self.assertEqual(result["reasons"], ["unknown_paths"])
        self.assertEqual(result["unknown_paths"], ["new-language/source.xyz"])
        self.assertIn("test_wasm", selected(result))

    def test_bad_metadata_selects_full(self):
        for total in [None, "13", True, -1]:
            with self.subTest(total=total):
                result = select(total=total)
                self.assertEqual(result["reasons"], ["detection_failed"])
                self.assertIn("check_rust", selected(result))

    def test_detector_failure_and_missing_files_select_full(self):
        for kwargs in [{"outcome": "failure"}, {"files": None}]:
            with self.subTest(kwargs=str(kwargs)):
                result = select(**kwargs)
                self.assertEqual(result["reasons"], ["detection_failed"])
                self.assertIn("test_android", selected(result))

    def test_draft_pushes_run_only_draft_suites(self):
        result = select(["proto/a.proto"], payload=event(draft=True))
        self.assertEqual(result["event_kind"], "draft_pr_push")
        self.assertEqual(selected(result), eligible("draft_pr_push"))
        self.assertIn("lint_workspace", selected(result))
        self.assertEqual(result["suites"]["test"], [])
        self.assertEqual(result["advisory"], [])

    def test_nix_evaluation_skips_drafts_and_follows_files_that_nix_reads(self):
        # Evaluation reads these files; a broken one fails `nix eval`.
        read = [
            "nix/package/ios.nix",
            "crates/xmtp_mls/Cargo.toml",
            "Cargo.lock",
            "webdriver.json",
            "package.json",
            "nix/sdk-platforms.json",
            "nix/package/example-android-apk-deps.json",
            "sdks/android/gradle.properties",
            "sdks/android/gradle/wrapper/gradle-wrapper.properties",
            "sdks/react-native/package.json",
            "sdks/react-native/test/consumers/native-toolchain.json",
            "sdks/react-native/test/consumers/ios-downloads.json",
            "sdks/react-native/test/consumers/bare/template-source.json",
            "sdks/react-native/test/consumers/expo/android/gradle/dependencies.json",
        ]
        for path in read:
            with self.subTest(path=path):
                self.assertIn("nix_eval", selected(select([path])))
                draft = select([path], payload=event(draft=True))
                self.assertNotIn("nix_eval", selected(draft))
        self.assertNotIn("nix_eval", selected(select(["crates/xmtp_mls/src/lib.rs"])))
        self.assertIn("nix_eval", selected(select(name="push")))
        # A deleted file can leave a Nix file that names it.
        deleted = select(["dev/old-script"], deleted=["dev/old-script"])
        self.assertIn("nix_eval", selected(deleted))

    def test_forks_skip_disabled_suites(self):
        result = select(["proto/a.proto"], payload=event(fork=True))
        self.assertNotIn("test_ios", selected(result))
        self.assertIn("test_android", selected(result))
        self.assertIn("test_ios", result["policy_excluded"])

    def test_covered_suite_gives_way_only_to_an_equal_or_stronger_tier(self):
        # docs_rust is covered by docs_site. Make docs_site advisory for a
        # path that only its advisory filter matches.
        suites = copy.deepcopy(SELECTOR.SUITES)
        suites["docs_site"]["paths"] = ["docs/**"]
        suites["docs_site"]["advisory"]["paths"] = ["apps/error_glossary/**"]
        manifest = SELECTOR.MANIFEST | {"suites": suites}
        with mock.patch.multiple(SELECTOR, MANIFEST=manifest, SUITES=suites):
            rust = "apps/error_glossary/src/main.rs"
            result = select([rust])
            self.assertIn("docs_rust", selected(result))
            self.assertEqual(advisory(result), {"docs_site"})
            result = select([rust, "docs/guide.md"])
            self.assertNotIn("docs_rust", selected(result))
            self.assertIn("docs_site", selected(result))

    def test_summary_shows_paths_as_text(self):
        # A changed path cannot add HTML or Markdown lines to the step summary.
        path = "proto/<img src=x>\n## CI_PATH_DATA.proto"
        text = SELECTOR.summary(select([path]))
        self.assertNotIn("<img", text)
        lines = text.splitlines()
        self.assertEqual([x for x in lines if x.startswith("## CI_PATH")], [])
        self.assertIn("&lt;img src=x&gt;\\n## CI_PATH_DATA.proto", text)

    def test_summary_shows_matching_labels_as_text(self):
        label = "<img src=x>\n## Label | match"
        with mock.patch.dict(SELECTOR.SUITES["test_example_ios"], labels=[label]):
            result = select(payload=event(labels=[label]))
        text = SELECTOR.summary(result)
        self.assertIn("test_example_ios", result["matched_suites"])
        self.assertIn("### Matching PR labels", text)
        self.assertIn("&lt;img src=x&gt;\\n## Label &#124; match", text)
        self.assertNotIn("<img", text)
        self.assertFalse(any(line.startswith("## Label") for line in text.splitlines()))


def with_groups():
    """Two groups: test_android waits for both, test_example_android for one."""
    manifest = copy.deepcopy(SELECTOR.MANIFEST)
    manifest["products"] = {
        "alpha": {"cargo_roots": [], "crates": [], "paths": ["crates/xmtp_sdk/**"]},
        "beta": {
            "cargo_roots": [],
            "crates": [],
            "paths": ["sdks/android/**", "!sdks/android/**/*.md"],
        },
    }
    for name, suite in manifest["suites"].items():
        suite["products"] = []
    manifest["suites"]["test_android"]["products"] = ["alpha", "beta"]
    manifest["suites"]["test_example_android"]["products"] = ["beta"]
    return mock.patch.multiple(SELECTOR, MANIFEST=manifest, SUITES=manifest["suites"])


class ProductSelectionTests(unittest.TestCase):
    kotlin = "sdks/android/library/src/main/java/Client.kt"
    sdk = "crates/xmtp_sdk/src/client.rs"

    def test_groups_need_a_waiting_suite_and_a_matching_path(self):
        with with_groups():
            for files, expected in [
                ([self.kotlin], ["beta"]),
                # One suite that waits for two groups.
                ([self.kotlin, self.sdk], ["alpha", "beta"]),
                # test_android runs, but no path matches beta's filters.
                ([self.sdk], ["alpha"]),
                ([self.sdk.replace("client.rs", "AGENTS.md")], []),
                (["sdks/android/README.md"], []),
                # No selected suite waits for a group.
                (["crates/xmtp_mls/src/client.rs"], []),
                (["apps/example-android/app/build.gradle"], []),
            ]:
                with self.subTest(files=files):
                    result = select(files)
                    self.assertEqual(result["products"], expected)

    def test_groups_build_only_with_the_pr_cache_token(self):
        # Dependabot runs get the token once it is a Dependabot secret, and
        # then build products like a same-repository pull request. Without it
        # (forks, or Dependabot before the secret exists) no group runs and
        # the suites build the products themselves.
        dependabot = event() | {"sender": {"login": "dependabot[bot]"}}
        with with_groups():
            for payload, token, expected in [
                (event(), True, ["beta"]),
                (dependabot, True, ["beta"]),
                (dependabot, False, []),
                (event(fork=True), False, []),
            ]:
                with self.subTest(payload=payload.get("sender"), token=token):
                    result = select([self.kotlin], payload=payload, token=token)
                    self.assertEqual(result["products"], expected)
                    self.assertIn("test_android", selected(result))
            self.assertEqual(select(name="push", token=False)["products"], [])
            self.assertEqual(select(name="push")["products"], ["alpha", "beta"])

    def test_label_selected_suite_needs_matching_product_inputs(self):
        with with_groups():
            SELECTOR.MANIFEST["products"]["beta"]["paths"] = ["docs/product-input.md"]
            for paths, labels, expected in [
                (["docs/product-input.md"], [], []),
                (["docs/guide.md"], ["android"], []),
                (["docs/product-input.md"], ["android"], ["beta"]),
            ]:
                with self.subTest(paths=paths, labels=labels):
                    result = select(paths, payload=event(labels=labels))
                    self.assertEqual(result["products"], expected)


def dispatch(suites):
    return {"repository": {"full_name": "xmtp/libxmtp"}, "inputs": {"suites": suites}}


class ManualRunTests(unittest.TestCase):
    def test_suites_input_runs_exactly_the_listed_suites_as_blocking(self):
        for text, lint, test in [
            ("test_ios", [], ["test_ios"]),
            (" test_ios ,lint_proto,test_ios", ["lint_proto"], ["test_ios"]),
            # Manual runs ignore run_on and the advisory tier.
            ("nix_eval,test_example_ios", ["nix_eval"], ["test_example_ios"]),
        ]:
            with self.subTest(text=text):
                result = select(name="workflow_dispatch", payload=dispatch(text))
                self.assertEqual(result["reasons"], ["explicit_request"])
                self.assertEqual(result["suites"], {"lint": lint, "test": test})
                self.assertEqual(result["advisory"], [])
        result = select(name="workflow_dispatch", payload=dispatch("test_android"))
        self.assertEqual(result["products"], ["sdk_android"])

    def test_empty_suites_input_runs_every_suite(self):
        everything = set(SELECTOR.SUITES) - {"docs_rust"}
        for payload in (dispatch(""), dispatch(" , "), {"inputs": {}}, {}):
            with self.subTest(payload=payload):
                result = select(name="workflow_dispatch", payload=payload)
                self.assertEqual(selected(result), everything)

    def test_unknown_suites_fail_with_their_names(self):
        with self.assertRaisesRegex(
            SELECTOR.RequestError, r'"test_nope", "x\\ny"\. Use suite keys'
        ):
            select(
                name="workflow_dispatch", payload=dispatch("test_ios,test_nope,x\ny")
            )


class MatchTests(unittest.TestCase):
    def test_glob_semantics(self):
        for pattern, path, expected in [
            ("**/AGENTS.md", "AGENTS.md", True),
            ("**/AGENTS.md", "crates/a/AGENTS.md", True),
            ("crates/*.rs", "crates/a/b.rs", False),
            ("crates/**", "crates/a/b.rs", True),
            ("nix/package/backend{,-ci}.nix", "nix/package/backend-ci.nix", True),
            ("nix/package/backend{,-ci}.nix", "nix/package/backend-cix.nix", False),
            ("sdks/{node,browser}/**/*.{ts,tsx}", "sdks/node/src/a/b.tsx", True),
            ("a?.md", "ab.md", True),
            ("a?.md", "a/.md", False),
            ("docs/a+b.md", "docs/aab.md", False),
            ("docs/**", "docs/a\nb.md", True),
        ]:
            with self.subTest(pattern=pattern, path=path):
                self.assertEqual(SELECTOR.glob_match(pattern, path), expected)


class MetadataTests(unittest.TestCase):
    def test_api_total_is_checked_against_the_event_head(self):
        payload = event()
        del payload["pull_request"]["changed_files"]
        env = {
            "GITHUB_API_URL": "https://api.github.com",
            "GITHUB_REPOSITORY": "xmtp/libxmtp",
            "GITHUB_TOKEN": "test-token",
        }
        for head, total, expected in [
            ("event-head", 3001, (3001, None)),
            ("new-head", 1, (None, "The PR head changed after this event.")),
        ]:
            with self.subTest(head=head):
                response = mock.MagicMock()
                response.__enter__.return_value.read.return_value = json.dumps(
                    {"head": {"sha": head}, "changed_files": total}
                )
                with mock.patch.object(
                    SELECTOR, "urlopen", return_value=response
                ) as fetch:
                    self.assertEqual(SELECTOR.pr_changed_files(payload, env), expected)
                self.assertEqual(fetch.call_args.kwargs["timeout"], 20)


def load(path):
    return yaml.safe_load((ROOT / path).read_text())


class WorkflowTests(unittest.TestCase):
    workflow = load(".github/workflows/ci.yml")
    jobs = workflow["jobs"]

    def run_select(self, data, payload=None, name="pull_request"):
        step = next(
            s for s in self.jobs["detect-changes"]["steps"] if s.get("id") == "select"
        )
        script = re.sub(
            r"\$\{\{ toJSON\(steps.paths.outputs.(\w+)\) \}\}",
            lambda m: json.dumps(data[m[1]]),
            step["run"],
        )
        # The selected workflow command must run from the checkout root.
        script = script.replace(
            "python3 dev/ci-select", f'python3 "{ROOT / "dev/ci-select"}"'
        )
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            (temp / "event.json").write_text(
                json.dumps(event() if payload is None else payload)
            )
            env = os.environ | {
                "GITHUB_EVENT_PATH": str(temp / "event.json"),
                "GITHUB_EVENT_NAME": name,
                "PATHS_OUTCOME": "success",
                "PR_CACHE_TOKEN": "true",
                "GITHUB_OUTPUT": str(temp / "outputs"),
                "GITHUB_STEP_SUMMARY": str(temp / "summary"),
            }
            run = subprocess.run(
                ["bash", "-c", script],
                cwd=temp,
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertFalse((temp / "injected").exists())
            outputs = temp / "outputs"
            values = dict(
                line.split("=", 1)
                for line in (
                    outputs.read_text() if outputs.exists() else ""
                ).splitlines()
            )
            return run, values, (temp / "summary").read_text()

    def test_workflow_passes_json_as_data(self):
        path = 'docs/quote"\nCI_PATH_DATA\n$(touch injected)\n<script>.md'
        data = {
            "changes": json.dumps(["all_files"]),
            "all_files_files": json.dumps([path]),
        }
        run, values, text = self.run_select(data)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual(json.loads(values["test_suites"]), ["docs_site"])
        self.assertEqual(json.loads(values["products"]), [])
        self.assertEqual(json.loads(values["advisory_suites"]), [])
        # The path is not in the checkout, so it counts as deleted.
        lint = json.loads(values["lint_suites"])
        self.assertEqual(set(lint), {"docs_quality", "lint_config", "nix_eval"})
        self.assertIn("docs_site", text)
        outputs = self.jobs["detect-changes"]["outputs"]
        self.assertEqual(
            set(outputs), {"lint_suites", "test_suites", "advisory_suites", "products"}
        )

    def test_summary_names_blocking_and_advisory_suites(self):
        data = {
            "changes": '["all_files"]',
            "all_files_files": '["sdks/ios/Sources/Client.swift"]',
        }
        run, values, text = self.run_select(data)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual(json.loads(values["advisory_suites"]), ["test_example_ios"])
        self.assertIn("test_ios", json.loads(values["test_suites"]))
        blocking, advisory_part = text.split("### Advisory suites", 1)
        self.assertIn(
            "<code>test_ios</code>", blocking.split("### Blocking test suites")[1]
        )
        self.assertIn("<code>test_example_ios</code>", advisory_part.split("###")[0])

    def test_manual_run_reads_the_suites_input(self):
        triggers = self.workflow["on"]["workflow_dispatch"]["inputs"]["suites"]
        self.assertEqual((triggers["type"], triggers["required"]), ("string", False))
        data = {"changes": "null", "all_files_files": "null"}
        run, values, _ = self.run_select(
            data, {"inputs": {"suites": "test_ios"}}, "workflow_dispatch"
        )
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual(json.loads(values["test_suites"]), ["test_ios"])
        self.assertEqual(json.loads(values["lint_suites"]), [])
        run, values, text = self.run_select(
            data, {"inputs": {"suites": "test_ios,<b>nope"}}, "workflow_dispatch"
        )
        self.assertEqual(run.returncode, 1)
        self.assertEqual(values, {})
        self.assertIn('Unknown suites in the suites input: "<b>nope"', run.stdout)
        self.assertIn("&lt;b&gt;nope", text)
        self.assertNotIn("<b>", text)

    def test_label_events_recalculate_example_selection(self):
        triggers = self.workflow["on"]["pull_request"]["types"]
        examples = {"test_example_ios", "test_example_android"}
        data = {"changes": '["all_files"]', "all_files_files": '["docs/guide.md"]'}
        for action in ("labeled", "unlabeled"):
            self.assertIn(action, triggers)
            for label, suite in (
                ("iOS", "test_example_ios"),
                ("android", "test_example_android"),
            ):
                with self.subTest(action=action, label=label):
                    payload = event(labels=[label] if action == "labeled" else [])
                    payload.update(action=action, label={"name": label})
                    run, values, summary = self.run_select(data, payload)
                    self.assertEqual(run.returncode, 0, run.stderr)
                    self.assertEqual(
                        set(json.loads(values["test_suites"])) & examples,
                        {suite} if action == "labeled" else set(),
                    )
                    if action == "labeled":
                        self.assertIn(f"<code>{label}</code>", summary)

    def gate(self, phase, selected, results, detected="success", products=()):
        """Run a gate step the way GitHub runs it: `needs` holds only the jobs
        the gate needs."""
        gate = self.jobs[phase]
        (step,) = gate["steps"]
        needs = {job: {"result": "skipped"} for job in gate["needs"]}
        needs["detect-changes"] = {"result": detected}
        needs.update(
            (job, {"result": result}) for job, result in results.items() if job in needs
        )
        values = {
            "DETECTED": detected,
            "SELECTED": json.dumps(selected),
            "PRODUCTS": json.dumps(list(products)),
            "RESULTS": json.dumps(needs),
        }
        env = {}
        for key, raw in step["env"].items():
            env[key] = values[key] if "${{" in str(raw) else str(raw).strip("'")
        return subprocess.run(
            ["bash", "-c", step["run"]],
            env=os.environ | env,
            capture_output=True,
            text=True,
        )

    def test_gates_need_detection_and_only_their_blocking_callers(self):
        for phase in ("lint", "test"):
            with self.subTest(phase=phase):
                gate = self.jobs[phase]
                self.assertEqual(gate["if"], "always()")
                callers = [
                    job
                    for job, spec in self.jobs.items()
                    if f"outputs.{phase}_suites)" in str(spec.get("if", ""))
                ]
                products = [j for j in self.jobs if j.startswith("products_")]
                expected = ["detect-changes"] + (products if phase == "test" else [])
                self.assertEqual(gate["needs"], expected + callers)
                self.assertEqual(
                    set(callers),
                    {n for n, s in SELECTOR.SUITES.items() if s["phase"] == phase},
                )
                self.assertFalse([job for job in gate["needs"] if "advisory" in job])

    def test_gate_requires_detection_and_every_selected_blocking_suite(self):
        for state, expected in [
            ("success", 0),
            ("failure", 1),
            ("skipped", 1),
            ("cancelled", 1),
        ]:
            with self.subTest(state=state):
                run = self.gate("test", ["docs_site"], {"docs_site": state})
                self.assertEqual(run.returncode, expected, run.stdout)
        # Unselected suites are skipped and do not count.
        self.assertEqual(self.gate("test", [], {}).returncode, 0)
        self.assertEqual(
            self.gate("lint", ["lint_proto"], {"lint_proto": "success"}).returncode, 0
        )
        self.assertEqual(
            self.gate("lint", ["lint_proto"], {"lint_proto": "skipped"}).returncode, 1
        )
        for detected in ("failure", "cancelled", "skipped"):
            with self.subTest(detected=detected):
                run = self.gate("lint", [], {}, detected=detected)
                self.assertEqual(run.returncode, 1)
                self.assertIn(f"Select suites: {detected}", run.stdout)

    def test_gate_ignores_advisory_callers(self):
        # An advisory caller is not a need, so its failure never reaches the
        # gate; the blocking caller of the same suite was skipped.
        run = self.gate(
            "test",
            ["test_ios"],
            {
                "test_ios": "success",
                "test_example_ios": "skipped",
                "test_example_ios_advisory": "failure",
            },
        )
        self.assertEqual(run.returncode, 0, run.stdout)

    def test_gate_counts_a_products_job_only_for_blocking_suites(self):
        broken = {"products_sdk_android": "failure", "test_android": "skipped"}
        run = self.gate("test", ["test_android"], broken, products=["sdk_android"])
        self.assertEqual(run.returncode, 1)
        self.assertIn("products_sdk_android: failure; the suites", run.stdout)
        self.assertNotIn("test_android: skipped", run.stdout)
        # Only the advisory Messenger run waits for the group.
        run = self.gate(
            "test",
            [],
            {
                "products_sdk_android": "failure",
                "test_example_android_advisory": "skipped",
            },
            products=["sdk_android"],
        )
        self.assertEqual(run.returncode, 0, run.stdout)
        # A group that did not run: the suite built the products itself.
        run = self.gate(
            "test",
            ["test_android"],
            {"products_sdk_android": "skipped", "test_android": "success"},
        )
        self.assertEqual(run.returncode, 0, run.stdout)


if __name__ == "__main__":
    unittest.main()
