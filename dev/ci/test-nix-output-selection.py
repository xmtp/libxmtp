#!/usr/bin/env python3
"""Check warming selection, ancestor proof, and real workflow command guards."""

import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "nix_selector", Path(__file__).with_name("select-nix-outputs.py")
)
selector = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(selector)
BASE = "a" * 40
HEAD = "b" * 40
REPOSITORY = "xmtp/libxmtp"
REF = "refs/heads/self-hosted"
WORKFLOW_REF = f"{REPOSITORY}/{selector.WORKFLOW}@{REF}"


class SelectionTests(unittest.TestCase):
    def test_proved_prose_and_handwritten_web_inputs_skip_warming(self):
        for name in (
            "docs/guide.md",
            "README.md",
            "sdks/node/src/Client.ts",
            "sdks/browser/test/client.test.mts",
            "sdks/agent/src/index.ts",
            "apps/cli/src/index.ts",
            "apps/docs/src/components/example.tsx",
            "apps/web-chat/src/styles.css",
            "dev/release-tools/src/main.ts",
        ):
            with self.subTest(name=name):
                self.assertFalse(selector.select([name], verified=True)["full"])

    def test_compile_embedded_unknown_and_config_inputs_keep_full_graph(self):
        for name in (
            *selector.EMBEDDED_DOCS,
            "crates/xmtp_sdk/src/lib.rs",
            "apps/xmtp_sdk_bindgen/runtime/ts/host.ts",
            "nix/lib/filesets.nix",
            "Cargo.lock",
            "flake.lock",
            "rust-toolchain.toml",
            "sdks/node/package.json",
            "apps/docs/astro.config.mjs",
            "apps/unknown/file.ts",
            "docs/diagram.custom",
            "new-input/file.custom",
        ):
            with self.subTest(name=name):
                self.assertTrue(selector.select([name], verified=True)["full"])

    def test_missing_proof_tags_manual_and_mixed_changes_are_full(self):
        self.assertTrue(selector.select(["docs/guide.md"])["full"])
        self.assertTrue(
            selector.select(["docs/guide.md"], True, "workflow_dispatch", REF)["full"]
        )
        self.assertTrue(selector.select([], True, "push", "refs/tags/v1.0.0")["full"])
        self.assertTrue(selector.select(["docs/guide.md", "Cargo.toml"], True)["full"])
        self.assertFalse(selector.select([], True)["full"])

    def test_invalid_changed_path_data_is_rejected(self):
        for paths in (
            None,
            {},
            ["../escape"],
            ["/absolute"],
            [""],
            ["docs//guide.md"],
            ["docs\\guide.md"],
            [False],
        ):
            with self.subTest(paths=paths), self.assertRaises(ValueError):
                selector.select(paths, True)


class ProofTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.event = Path(self.temp.name) / "event.json"
        self.event.write_text(json.dumps({"before": BASE, "after": HEAD, "ref": REF}))
        self.workflow = {"id": 42, "path": selector.WORKFLOW}
        self.success = {
            "id": 100,
            "workflow_id": 42,
            "path": selector.WORKFLOW,
            "head_sha": BASE,
            "head_branch": "self-hosted",
            "event": "push",
            "status": "completed",
            "conclusion": "success",
            "run_attempt": 1,
        }
        self.records = [self.success]
        self.calls = []

    def api(self, repository, endpoint):
        self.calls.append(endpoint)
        self.assertEqual(repository, REPOSITORY)
        return (
            self.workflow
            if endpoint == "actions/workflows/fh-cache.yml"
            else {"workflow_runs": self.records}
        )

    def proof(self, **kwargs):
        defaults = {
            "event": "push",
            "ref": REF,
            "event_file": self.event,
            "repository": REPOSITORY,
            "workflow_ref": WORKFLOW_REF,
            "expected_sha": HEAD,
        }
        defaults.update(kwargs)

        def command(args):
            if args[1:3] == ["rev-parse", "HEAD"]:
                return (HEAD + "\n").encode()
            return b"docs/guide.md\0"

        with (
            patch.object(selector, "command", side_effect=command),
            patch.object(selector, "graph_matches", return_value=True),
            patch.object(selector, "input_contract_matches", return_value=True),
            patch.object(selector, "github", side_effect=self.api),
            patch.object(selector.subprocess, "run"),
        ):
            return selector.changed_inputs(**defaults)

    def test_current_workflow_success_proves_the_exact_push_ancestor(self):
        paths, verified, _ = self.proof()
        self.assertTrue(verified)
        self.assertEqual(paths, ["docs/guide.md"])
        self.assertIn(f"head_sha={BASE}", self.calls[-1])
        self.assertIn("event=push", self.calls[-1])

    def test_latest_failure_cancel_pending_or_rerun_does_not_use_old_success(self):
        for state in ("failure", "cancelled", None):
            with self.subTest(state=state):
                self.records = [
                    self.success,
                    {**self.success, "id": 101, "conclusion": state},
                ]
                self.assertFalse(self.proof()[1])
        self.records = [{**self.success, "run_attempt": 2, "conclusion": "failure"}]
        self.assertFalse(self.proof()[1])

    def test_unrelated_workflow_branch_head_or_event_never_proves_coverage(self):
        for field, value in (
            ("workflow_id", 43),
            ("path", ".github/workflows/test.yml"),
            ("head_sha", HEAD),
            ("head_branch", "main"),
            ("event", "pull_request"),
            ("status", "in_progress"),
            ("run_attempt", 0),
            ("run_attempt", True),
        ):
            with self.subTest(field=field):
                self.records = [{**self.success, field: value}]
                self.assertFalse(self.proof()[1])
        self.workflow = {"id": 42, "path": ".github/workflows/test.yml"}
        self.assertFalse(self.proof()[1])

    def test_graph_input_contract_and_api_failures_select_full(self):
        for function in ("graph_matches", "input_contract_matches"):
            with (
                patch.object(selector, "command", return_value=(HEAD + "\n").encode()),
                patch.object(selector, "graph_matches", return_value=True),
                patch.object(selector, "input_contract_matches", return_value=True),
                patch.object(selector, function, return_value=False),
                patch.object(selector.subprocess, "run"),
                patch.object(selector, "github", side_effect=self.api),
            ):
                self.assertFalse(
                    selector.changed_inputs(
                        "push", REF, self.event, REPOSITORY, WORKFLOW_REF, HEAD
                    )[1]
                )
        with (
            patch.object(selector, "command", return_value=(HEAD + "\n").encode()),
            patch.object(selector, "graph_matches", return_value=True),
            patch.object(selector, "input_contract_matches", return_value=True),
            patch.object(selector.subprocess, "run"),
            patch.object(
                selector, "github", side_effect=subprocess.TimeoutExpired("gh", 20)
            ),
        ):
            self.assertFalse(
                selector.changed_inputs(
                    "push", REF, self.event, REPOSITORY, WORKFLOW_REF, HEAD
                )[1]
            )

    def test_malformed_event_identity_and_nonancestor_are_full(self):
        for payload in (
            [],
            {},
            {"before": "0" * 40, "after": HEAD, "ref": REF},
            {"before": BASE, "after": BASE, "ref": REF},
            {"before": BASE, "after": HEAD, "ref": "refs/heads/main"},
        ):
            self.event.write_text(json.dumps(payload))
            self.assertFalse(self.proof()[1])
        self.event.write_text(json.dumps({"before": BASE, "after": HEAD, "ref": REF}))
        self.assertFalse(
            self.proof(workflow_ref="other/repo/.github/workflows/fh-cache.yml@" + REF)[
                1
            ]
        )
        with (
            patch.object(selector, "command", return_value=(HEAD + "\n").encode()),
            patch.object(
                selector.subprocess,
                "run",
                side_effect=subprocess.CalledProcessError(1, "git"),
            ),
        ):
            self.assertFalse(
                selector.changed_inputs(
                    "push", REF, self.event, REPOSITORY, WORKFLOW_REF, HEAD
                )[1]
            )


class ContractTests(unittest.TestCase):
    def test_source_input_contract_blocks_new_reads_even_after_a_successful_push(self):
        records = b"100644 blob " + b"a" * 40 + b"\tapps/backend/src/main.rs\n"
        with patch.object(selector, "command", return_value=records):
            self.assertFalse(selector.input_contract_matches(HEAD))
        # Prose is outside the audit pin, but a new Rust/build.rs reader is inside it.
        import hashlib

        expected = hashlib.sha256(records).hexdigest()
        with (
            patch.object(selector, "AUDITED_INPUT_CONTRACT", expected),
            patch.object(
                selector,
                "command",
                return_value=records
                + b"100644 blob "
                + b"b" * 40
                + b"\tdocs/guide.md\n",
            ),
        ):
            self.assertTrue(selector.input_contract_matches(HEAD))
        for name in (
            "crates/new/build.rs",
            "apps/new/src/main.rs",
            "nix/new-input.nix",
            "crates/xmtp_sdk/dev/record-generated.py",
            "dev/backend/read-inputs",
            "dev/ci/backend-products.py",
            "apps/other/generate.mjs",
            "Justfile",
            ".new-build-config",
        ):
            changed = (
                records + b"100644 blob " + b"b" * 40 + b"\t" + name.encode() + b"\n"
            )
            with (
                patch.object(selector, "AUDITED_INPUT_CONTRACT", expected),
                patch.object(selector, "command", return_value=changed),
            ):
                self.assertFalse(selector.input_contract_matches(HEAD))

    def test_selector_normalization_excludes_only_the_literal_pin_value(self):
        first = (
            b"AUDITED_INPUT_CONTRACT = 'first'\ndef reader():\n    return 'original'\n"
        )
        second = first.replace(b"'first'", b"'second'")
        self.assertEqual(
            selector.normalized_selector(first), selector.normalized_selector(second)
        )
        self.assertNotEqual(
            selector.normalized_selector(first),
            selector.normalized_selector(first.replace(b"'original'", b"'new-reader'")),
        )
        for invalid in (
            b"AUDITED_INPUT_CONTRACT = load_pin()\n",
            b"AUDITED_INPUT_CONTRACT = OTHER = 'pin'\n",
            b"AUDITED_INPUT_CONTRACT = 'one'\nAUDITED_INPUT_CONTRACT = 'two'\n",
        ):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                selector.normalized_selector(invalid)

    def test_selector_blob_and_other_dev_scripts_are_part_of_contract(self):
        records = (
            b"100644 blob " + b"a" * 40 + b"\tdev/ci/select-nix-outputs.py\n"
            b"100644 blob " + b"b" * 40 + b"\tdev/ci/backend-products.py\n"
        )
        source = (
            b"AUDITED_INPUT_CONTRACT = 'one'\ndef reader():\n    return 'original'\n"
        )

        def fingerprint(data):
            def command(args):
                return records if args[1] == "ls-tree" else data

            with patch.object(selector, "command", side_effect=command):
                return selector.contract_fingerprint(HEAD)

        before = fingerprint(source)
        self.assertEqual(before, fingerprint(source.replace(b"'one'", b"'two'")))
        self.assertNotEqual(
            before, fingerprint(source.replace(b"'original'", b"'new-reader'"))
        )

    def test_graph_comparison_includes_added_and_deleted_paths(self):
        with patch.object(
            selector, "command", return_value=b"dev/ci/removed.py\0"
        ) as command:
            self.assertFalse(selector.graph_matches(BASE, HEAD))
            self.assertEqual(command.call_args.args[0][-3:], list(selector.GRAPH))


class WorkflowTests(unittest.TestCase):
    def test_darwin_never_uses_source_only_proof_to_skip_host_validation(self):
        workflow = (ROOT / ".github/workflows/fh-cache.yml").read_text()
        sections = dict(
            re.findall(r"- name: ([^\n]+)\n(.*?)(?=\n      - |\Z)", workflow, re.S)
        )
        for name in (
            "Install omnix",
            "Build all flake outputs",
            "Pin outputs to cachix",
        ):
            self.assertIn("|| runner.os == 'macOS'", sections[name])
        source_selection = selector.select(["docs/guide.md"], verified=True)
        self.assertFalse(source_selection["full"])
        # The source proof has no Apple SDK identity. All host states keep the
        # original Darwin build path, including an unchanged or changed SDK.
        for sdk_state in ("unknown", "current", "changed"):
            with self.subTest(sdk_state=sdk_state):
                runner_os = "macOS"
                self.assertTrue(source_selection["full"] or runner_os == "macOS")

    def test_actual_omnix_command_runs_only_when_full_is_selected(self):
        workflow = (ROOT / ".github/workflows/fh-cache.yml").read_text()
        sections = dict(
            re.findall(r"- name: ([^\n]+)\n(.*?)(?=\n      - |\Z)", workflow, re.S)
        )
        build = sections["Build all flake outputs"]
        expected_guard = (
            "if: needs.detect-changes.outputs.full == 'true' || runner.os == 'macOS'"
        )
        for name in (
            "Install omnix",
            "Build all flake outputs",
            "Pin outputs to cachix",
        ):
            self.assertIn(expected_guard, sections[name])
        self.assertNotIn("if:", sections["Check source formatting"])
        self.assertIn(
            "nix fmt -- --fail-on-change", sections["Check source formatting"]
        )
        command = re.search(r"run: (om ci run[^\n]+)", build).group(1)
        self.assertEqual(
            command,
            "om ci run --include-all-dependencies --results=om.json -- --keep-going",
        )
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            stub = folder / "om"
            stub.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$OM_PROOF"\n')
            stub.chmod(0o755)
            proof = folder / "proof"
            environment = dict(
                os.environ,
                PATH=str(folder) + os.pathsep + os.environ["PATH"],
                OM_PROOF=str(proof),
            )
            for full, runner_os in (
                (False, "Linux"),
                (True, "Linux"),
                (False, "macOS"),
                (True, "macOS"),
            ):
                proof.unlink(missing_ok=True)
                result = selector.select(["docs/guide.md"], verified=not full)
                if result["full"] or runner_os == "macOS":
                    subprocess.run(["bash", "-c", command], check=True, env=environment)
                self.assertEqual(proof.exists(), full or runner_os == "macOS")
                if full or runner_os == "macOS":
                    self.assertEqual(
                        proof.read_text().strip(),
                        "ci run --include-all-dependencies --results=om.json -- --keep-going",
                    )


if __name__ == "__main__":
    unittest.main()
