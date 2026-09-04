#!/usr/bin/env python3
"""Render the ignored-tests tracking issue body from nextest junit artifacts.

Usage: render-ignored-tests-report.py <artifacts_dir> <run_number> <run_url>

Reads <artifacts_dir>/<target>-results/{junit.xml,exit-code.txt} for each
target produced by .github/workflows/ignored-tests-tracker.yml and writes
the issue body markdown to stdout. The body embeds a machine-readable
failure count (`<!-- ignored-tests-failures: N -->`) that the next run uses
to detect regressions.
"""

import sys
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path

TARGETS = [
    ("native-default", "Native"),
    ("native-d14n", "Native + d14n"),
    ("wasm-default", "WASM"),
    ("wasm-d14n", "WASM + d14n"),
]

# nextest exit codes: 0 = all passed, 100 = some tests failed. Anything
# else means the build or test runner broke before producing results.
OK_EXIT_CODES = (0, 100)


def parse_target(results_dir: Path) -> dict:
    junit = results_dir / "junit.xml"
    exit_code_file = results_dir / "exit-code.txt"

    if not results_dir.is_dir():
        return {"status": "missing", "tests": []}

    exit_code = None
    if exit_code_file.is_file():
        try:
            exit_code = int(exit_code_file.read_text().strip())
        except ValueError:
            pass

    if not junit.is_file():
        return {"status": "build_failure", "exit_code": exit_code, "tests": []}

    try:
        root = ET.parse(junit).getroot()
    except ET.ParseError:
        return {"status": "build_failure", "exit_code": exit_code, "tests": []}

    tests = []
    for case in root.iter("testcase"):
        if case.find("skipped") is not None:
            continue
        name = case.get("name", "")
        classname = case.get("classname", "")
        full_name = f"{classname}::{name}" if classname else name
        failed = case.find("failure") is not None or case.find("error") is not None
        tests.append((full_name, failed))
    tests.sort()

    status = "ok"
    if exit_code is not None and exit_code not in OK_EXIT_CODES:
        status = "build_failure"
    return {"status": status, "exit_code": exit_code, "tests": tests}


def summary_status(result: dict) -> str:
    passed = sum(1 for _, failed in result["tests"] if not failed)
    failed = sum(1 for _, failed in result["tests"] if failed)
    if result["status"] == "missing":
        return ":warning: Missing"
    if result["status"] == "build_failure":
        return f":x: Build Failed (exit {result['exit_code']})"
    return f"{passed} :white_check_mark: / {failed} :x:"


def test_table_rows(result: dict) -> str:
    if result["status"] == "missing":
        return "| *(Artifact missing - job may have failed)* | - |"
    if result["status"] == "build_failure":
        return "| *(Build failed before tests could run)* | - |"
    if not result["tests"]:
        return "| *(No ignored tests found)* | - |"
    return "\n".join(
        f"| `{name}` | {':x: FAILED' if failed else ':white_check_mark: ok'} |"
        for name, failed in result["tests"]
    )


def main() -> None:
    if len(sys.argv) != 4:
        sys.exit(f"Usage: {sys.argv[0]} <artifacts_dir> <run_number> <run_url>")
    artifacts_dir, run_number, run_url = Path(sys.argv[1]), sys.argv[2], sys.argv[3]

    results = {
        key: parse_target(artifacts_dir / f"{key}-results") for key, _ in TARGETS
    }
    total_failures = sum(
        1 for r in results.values() for _, failed in r["tests"] if failed
    )
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")

    print("# Ignored Tests Status Report")
    print()
    print("This issue tracks the status of tests marked with `#[ignore]` in the")
    print("codebase. These tests are skipped during normal CI runs but are run")
    print("here on a schedule so they don't become permanent blindspots.")
    print()
    print(f"**Last Updated:** {timestamp}")
    print(f"**Run:** [#{run_number}]({run_url})")
    print(f"<!-- ignored-tests-failures: {total_failures} -->")
    print()
    print("## Summary")
    print()
    print("| Target | Status | Total |")
    print("|--------|--------|-------|")
    for key, label in TARGETS:
        print(
            f"| {label} | {summary_status(results[key])} | {len(results[key]['tests'])} |"
        )
    for key, label in TARGETS:
        result = results[key]
        print()
        print(f"## {label}")
        print()
        print("<details>")
        print(f"<summary>Test Results ({len(result['tests'])} tests)</summary>")
        print()
        print("| Test | Result |")
        print("|------|--------|")
        print(test_table_rows(result))
        print("</details>")
    print()
    print("---")
    print(
        "*This issue is automatically updated by the "
        "[Ignored Tests Tracker](.github/workflows/ignored-tests-tracker.yml) "
        "workflow.*"
    )


if __name__ == "__main__":
    main()
