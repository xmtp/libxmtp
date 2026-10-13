#!/usr/bin/env python3
"""Check dev/ci/metrics against saved GitHub API responses and job logs."""

import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "dev/ci/metrics"
LOADER = importlib.machinery.SourceFileLoader("ci_metrics", str(SCRIPT))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
METRICS = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(METRICS)

# A stand-in for `gh api`. It serves the fixture in CI_METRICS_FIXTURE and
# pages job lists as GitHub does: 30 jobs per page unless `per_page` is set,
# and only the first page unless `--paginate` is passed.
GH = r"""
import json, os, re, sys
from urllib.parse import parse_qs, urlsplit

args = sys.argv[1:]
if args == ["api", "--help"]:
    print("  --allow-escape-sequences  Allow ANSI escapes")
    raise SystemExit(0)
fixture = json.load(open(os.environ["CI_METRICS_FIXTURE"]))
flags = {arg for arg in args[1:-1]}
assert args[0] == "api" and flags <= {"--paginate", "--slurp", "--allow-escape-sequences"}, args
prefix = "repos/xmtp/libxmtp/"
assert args[-1].startswith(prefix), args
path = args[-1][len(prefix):]
url = urlsplit(path)
jobs = re.fullmatch(r"actions/runs/(\d+)/jobs", url.path)
log = re.fullmatch(r"actions/jobs/(\d+)/logs", url.path)
if jobs:
    listed = fixture["jobs"].get(jobs.group(1), [])
    size = int(parse_qs(url.query).get("per_page", ["30"])[0])
    pages = [listed[i:i + size] for i in range(0, len(listed), size)] or [[]]
    if "--paginate" not in flags:
        pages = pages[:1]
    pages = [{"total_count": len(listed), "jobs": page} for page in pages]
    print(json.dumps(pages if "--slurp" in flags else pages[0]))
elif log:
    if log.group(1) not in fixture["logs"]:
        print("gh: HTTP 404", file=sys.stderr)
        raise SystemExit(1)
    sys.stdout.write(fixture["logs"][log.group(1)])
elif path in fixture["api"]:
    response = fixture["api"][path]
    print(json.dumps([response] if "--slurp" in flags else response))
else:
    print("unexpected gh call: " + " ".join(args), file=sys.stderr)
    raise SystemExit(1)
"""

TRUNK = "c" * 40
PR_HEAD = "a" * 40
PR_BASE = "b" * 40


def job(
    job_id, name, start, seconds, label="blacksmith-16vcpu-ubuntu-2404", steps=None
):
    end = start + seconds
    return {
        "id": job_id,
        "name": name,
        "conclusion": "success",
        "labels": [label],
        "started_at": stamp(start),
        "completed_at": stamp(end),
        "steps": steps
        if steps is not None
        else [
            {
                "name": f"step of {name}",
                "started_at": stamp(start),
                "completed_at": stamp(end),
            }
        ],
    }


def stamp(offset):
    """An ISO time `offset` seconds after 12:00:00."""
    return f"2026-10-13T{12 + offset // 3600:02d}:{offset // 60 % 60:02d}:{offset % 60:02d}Z"


def push_run(run_id, sha=TRUNK, conclusion="success", tree="merge-tree"):
    return {
        "id": run_id,
        "event": "push",
        "conclusion": conclusion,
        "head_sha": sha,
        "path": ".github/workflows/ci.yml",
        "created_at": stamp(0),
        "run_started_at": stamp(0),
        "updated_at": stamp(900),
        "head_commit": {"tree_id": tree},
    }


TOOLS = "0" + "a" * 31
DEPS = "1" + "b" * 31
WASM_DRV = "4" + "c" * 31

NEXTEST_LOG = f"""\
2026-10-13T12:01:00.0000000Z ##[group]Run nix build .#nextest.x86_64-linux
2026-10-13T12:01:01.0000000Z these 2 paths will be fetched (2.74 MiB download, 69.74 MiB unpacked):
2026-10-13T12:01:01.0000000Z   /nix/store/r0fazmlv3sn45ig5wqiiasyynfksd47w-cargo-package-nextest-1.12.0-dev
2026-10-13T12:01:01.0000000Z   /nix/store/{TOOLS}-coverage-tools
2026-10-13T12:01:02.0000000Z copying path '/nix/store/r0fazmlv3sn45ig5wqiiasyynfksd47w-cargo-package-nextest-1.12.0-dev' from 'https://xmtp.cachix.org'...
2026-10-13T12:01:09.0000000Z these 41 derivations will be built:
2026-10-13T12:01:09.0000000Z   /nix/store/{DEPS}-cargo-package-nextest-deps-1.12.0-dev.drv
2026-10-13T12:01:10.0000000Z building '/nix/store/{DEPS}-cargo-package-nextest-deps-1.12.0-dev.drv'...
"""

# Nix plans to fetch the output, cannot substitute its closure, and builds it.
WASM_LOG = f"""\
2026-10-13T12:01:00.0000000Z \x1b[36;1mnix build --print-build-logs --no-link .#wasm-nextest.x86_64-linux\x1b[0m
2026-10-13T12:01:01.0000000Z this path will be fetched (0.00 MiB download, 0.00 MiB unpacked):
2026-10-13T12:01:01.0000000Z   /nix/store/3yvbq69nlc7s4nd5vfqahhddwiljhbpj-wasm-nextest-1.12.0-dev
2026-10-13T12:01:02.0000000Z building '/nix/store/{WASM_DRV}-wasm-nextest-1.12.0-dev.drv'...
"""


def merge_run_fixture(merge_tree="merge-tree", job_count=105):
    """A merge run whose PR's last run tested a merge commit with `merge_tree`."""
    jobs = [
        job(100 + i, f"Test suites / s{i} / Job {i}", 60, 60)
        for i in range(job_count - 3)
    ]
    jobs += [
        job(900, "Test suites / test_workspace / Test (Rust Workspace)", 60, 120),
        job(901, "Test suites / test_wasm / Test (WASM)", 60, 600),
        job(
            902,
            "Test suites / test_ios / Tests (iOS)",
            30,
            300,
            label="warp-macos-15-arm64-6x",
        ),
    ]
    return {
        "runs": {},
        "jobs": {"1": jobs, "2": [job(200, "detect-changes", 0, 30)], "3": []},
        "logs": {
            "200": f"2026-10-13T12:00:10.0000000Z HEAD is now at abcdef1 Merge {PR_HEAD} into {PR_BASE}\n",
            "900": NEXTEST_LOG,
            "901": WASM_LOG,
        },
        "api": {
            "actions/runs/1": push_run(1),
            f"commits/{TRUNK}?per_page=100": {
                "files": [
                    {"filename": "crates/xmtp_mls/src/a.rs", "status": "modified"}
                ]
            },
            f"commits/{TRUNK}/pulls": [
                {"merge_commit_sha": "d" * 40, "head": {"sha": "e" * 40}},
                {"merge_commit_sha": TRUNK, "head": {"sha": PR_HEAD}},
            ],
            # The newest PR run was replaced before it started, so it has no jobs.
            f"actions/workflows/ci.yml/runs?event=pull_request&head_sha={PR_HEAD}&per_page=100": {
                "workflow_runs": [
                    {"id": 2, "created_at": stamp(0)},
                    {"id": 3, "created_at": stamp(5)},
                ]
            },
            "commits/abcdef1": {"commit": {"tree": {"sha": merge_tree}}},
        },
    }


class Metrics(unittest.TestCase):
    def run_metrics(self, fixture, *args, check=True):
        with tempfile.TemporaryDirectory(prefix="ci-metrics-") as directory:
            folder = Path(directory)
            gh = folder / "gh"
            gh.write_text(f"#!{sys.executable}\n{GH}")
            gh.chmod(0o755)
            data = folder / "fixture.json"
            data.write_text(json.dumps(fixture))
            env = dict(
                os.environ,
                PATH=f"{folder}{os.pathsep}{os.environ['PATH']}",
                CI_METRICS_FIXTURE=str(data),
            )
            result = subprocess.run(
                [sys.executable, "-B", str(SCRIPT), *args],
                capture_output=True,
                text=True,
                env=env,
                check=False,
            )
        if check:
            self.assertEqual(result.returncode, 0, result.stderr)
        return result

    def test_merge_run_reads_every_job_page_and_its_pr_run(self):
        output = self.run_metrics(merge_run_fixture(), "runs", "1").stdout
        first = output.splitlines()[0]
        # 102 jobs of 1 min and 2 + 10 min of test jobs: 114 Linux runner-min.
        # The jobs fill two pages of 100, so without pagination the WASM and
        # iOS jobs are missing.
        self.assertIn("1 push success class=R wall=11.0 Linux=114.0 macOS=5.0", first)
        self.assertIn("tree=same", first)
        self.assertIn("nextest=fetched wasm=built", first)
        self.assertIn(
            "     600s Test (WASM) > step of Test suites / test_wasm / Test (WASM)",
            output,
        )
        self.assertIn(
            "median runner-min by class: Blacksmith 16 vCPU 114.0, Warp macOS 5.0",
            output,
        )
        self.assertIn("merge tree equals the PR's last tested tree: 1 of 1", output)
        self.assertIn("merge runs that fetched the nextest output: 1 of 1", output)
        self.assertIn("merge runs that fetched the wasm output: 0 of 1", output)

    def test_merge_tree_compares_the_tested_merge_commit(self):
        output = self.run_metrics(
            merge_run_fixture(merge_tree="other-tree"), "runs", "1"
        ).stdout
        self.assertIn("tree=differs", output.splitlines()[0])

    def test_listing_counts_cancelled_runs_by_whether_they_started(self):
        fixture = merge_run_fixture()
        started = push_run(11, sha="1" * 40, conclusion="cancelled")
        replaced = push_run(12, sha="2" * 40, conclusion="cancelled")
        fixture["jobs"]["11"] = [job(1100, "detect-changes", 0, 30)]
        fixture["api"].update(
            {
                f"commits/{'1' * 40}?per_page=100": {
                    "files": [{"filename": "README.md", "status": "modified"}]
                },
                f"commits/{'2' * 40}?per_page=100": {
                    "files": [{"filename": "flake.nix", "status": "modified"}]
                },
                "actions/workflows/ci.yml/runs?per_page=100&status=completed&branch=main&event=push&page=1": {
                    "workflow_runs": [replaced, started, push_run(1), push_run(13)]
                },
            }
        )
        output = self.run_metrics(
            fixture, "runs", "--branch", "main", "--event", "push", "--limit", "1"
        ).stdout
        self.assertIn("12 push cancelled before it started class=N", output)
        self.assertIn("11 push cancelled in progress class=0", output)
        self.assertIn(
            "runs: 1 measured; cancelled in progress 1; cancelled before start 1",
            output,
        )
        self.assertNotIn("13 push", output)

    def test_class_filter_skips_other_classes(self):
        fixture = merge_run_fixture()
        output = self.run_metrics(fixture, "runs", "--class", "L", "1").stdout
        self.assertIn("runs: 0 measured", output)
        self.assertNotIn("1 push success", output)

    def test_change_classes(self):
        cases = {
            "L": [
                [{"filename": "Cargo.lock", "status": "modified"}],
                [{"filename": "crates/xmtp_mls/Cargo.toml", "status": "modified"}],
                [{"filename": ".cargo/config.toml", "status": "modified"}],
                [{"filename": "apps/backend/build.rs", "status": "modified"}],
                [{"filename": "proto/xmtp/mls.proto", "status": "modified"}],
                [{"filename": "crates/xmtp_db/src/lib.rs", "status": "added"}],
                [{"filename": "apps/cli/src/bin/tool.rs", "status": "removed"}],
                [
                    {
                        "filename": "crates/new/src/lib.rs",
                        "previous_filename": "crates/new/src/main.rs",
                        "status": "renamed",
                    }
                ],
                [
                    {"filename": "README.md", "status": "modified"},
                    {"filename": "Cargo.lock", "status": "modified"},
                ],
            ],
            "R": [
                [{"filename": "crates/xmtp_db/src/lib.rs", "status": "modified"}],
                [{"filename": "crates/xmtp_mls/src/groups/mod.rs", "status": "added"}],
                [{"filename": "sdks/build.rs", "status": "modified"}],
                [
                    {"filename": "flake.nix", "status": "modified"},
                    {"filename": "apps/a/src/x.rs", "status": "modified"},
                ],
            ],
            "N": [
                [{"filename": "nix/package/nextest.nix", "status": "modified"}],
                [
                    {"filename": "flake.lock", "status": "modified"},
                    {"filename": "README.md", "status": "modified"},
                ],
            ],
            "0": [
                [],
                [{"filename": "README.md", "status": "modified"}],
                [{"filename": "dev/ci/metrics", "status": "added"}],
            ],
        }
        for expected, diffs in cases.items():
            for files in diffs:
                with self.subTest(files=files):
                    self.assertEqual(METRICS.change_class(files), expected)

    def test_marker_prints_only_matching_lines(self):
        fixture = {
            "jobs": {},
            "api": {},
            "logs": {
                "7": (
                    "2026-10-13T12:00:00.0000000Z Summary [ 830.120s] 1418 tests run\n"
                    "2026-10-13T12:00:01.0000000Z secret-looking output\n"
                    "2026-10-13T12:00:02.0000000Z \x1b[32mSummary [ 1.0s] 2 tests run\x1b[0m\n"
                    "2026-10-13T12:00:03.0000000Z Summary [ 2.0s] 3 tests run\n"
                )
            },
        }
        result = self.run_metrics(fixture, "marker", "7", r"^Summary \[", "--max", "2")
        self.assertEqual(
            result.stdout.splitlines(),
            [
                "2026-10-13T12:00:00.0000000Z Summary [ 830.120s] 1418 tests run",
                "2026-10-13T12:00:02.0000000Z Summary [ 1.0s] 2 tests run",
            ],
        )
        self.assertIn("1 more matching lines not shown", result.stderr)

    def test_marker_fails_when_the_log_is_missing(self):
        result = self.run_metrics(
            {"jobs": {}, "api": {}, "logs": {}}, "marker", "8", "x", check=False
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("cannot read the log of job 8", result.stderr)


if __name__ == "__main__":
    unittest.main()
