#!/usr/bin/env python3
"""Check collected recovery cases, then run one isolated partition."""

import argparse
from collections import Counter
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
TEST_FILE = "test/streamRecovery.test.ts"
SUITE = "public message stream recovery > "


def case_map(path):
    groups = json.loads(path.read_text())
    if set(groups) != {"A", "B", "C"}:
        raise ValueError("Recovery groups must be A, B, and C")
    cases = [case for group in groups.values() for case in group]
    if len(cases) != 8 or len(set(cases)) != 8:
        raise ValueError("Each of the eight recovery cases must have one owner")
    return groups


def collected_cases(path):
    records = json.loads(path.read_text())
    if not isinstance(records, list):
        raise ValueError("Expected the Vitest collected test list")
    cases = []
    for record in records:
        if not record["file"].replace("\\", "/").endswith("/" + TEST_FILE):
            raise ValueError("Collected a different test file")
        name = record["name"]
        if not name.startswith(SUITE):
            raise ValueError(f"Unmapped recovery case: {name}")
        cases.append(name[len(SUITE) :])
    return cases


def validate(groups, cases):
    expected = Counter(case for group in groups.values() for case in group)
    actual = Counter(cases)
    if actual != expected:
        raise ValueError(
            f"Recovery inventory differs: missing={list((expected - actual).elements())}; "
            f"unmapped or duplicate={list((actual - expected).elements())}"
        )


def pattern(cases):
    # Vitest matches the title plus its suite names. Anchor the exact leaf title.
    return "(?:" + "|".join(re.escape(case) for case in cases) + ")$"


def validate_results(path, cases):
    report = json.loads(path.read_text())
    passed = []
    for result in report["testResults"]:
        for assertion in result["assertionResults"]:
            if assertion["status"] == "passed":
                passed.append(assertion["title"])
            elif assertion["status"] not in {"pending", "skipped"}:
                raise ValueError(f"Recovery case did not pass: {assertion['title']}")
    if Counter(passed) != Counter(cases):
        raise ValueError("Executed recovery cases differ from the selected partition")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check", "run"])
    parser.add_argument("--group", choices=["A", "B", "C", "serial"])
    parser.add_argument("--inventory", type=Path)
    parser.add_argument("--map", type=Path, default=ROOT / "dev/ci/recovery-cases.json")
    args = parser.parse_args()
    groups = case_map(args.map)
    if args.command == "check":
        if args.inventory is None:
            parser.error("check requires --inventory")
        validate(groups, collected_cases(args.inventory))
        print("Recovery inventory: all eight cases have one owner")
        return
    if args.group is None:
        parser.error("run requires --group")
    backend = os.environ.get("XMTP_RECOVERY_BACKEND_BINARY", "")
    if (
        not backend
        or not Path(backend).is_absolute()
        or not os.access(backend, os.X_OK)
    ):
        raise ValueError("Recovery requires an executable absolute backend binary path")
    if os.environ.get("XMTP_RECOVERY_BUDGET_TESTS", "0") != "0":
        raise ValueError("The two recovery budget cases are local-only")
    env = {**os.environ, "XMTP_RECOVERY_BUDGET_TESTS": "0"}
    output = ROOT / "target/recovery-results" / args.group
    output.mkdir(parents=True, exist_ok=True)
    inventory = output / "inventory.json"
    results = output / "results.json"
    # Remove old reports before invoking Vitest so a failure cannot reuse them.
    inventory.unlink(missing_ok=True)
    results.unlink(missing_ok=True)
    command = ["pnpm", "--filter", "@xmtp/node-sdk", "exec", "vitest"]
    subprocess.run(
        command + ["list", TEST_FILE, "--json=" + str(inventory)],
        cwd=ROOT,
        env=env,
        check=True,
    )
    validate(groups, collected_cases(inventory))
    cases = (
        [case for group in groups.values() for case in group]
        if args.group == "serial"
        else groups[args.group]
    )
    (output / "selected.json").write_text(json.dumps(cases, indent=2) + "\n")
    subprocess.run(
        command
        + [
            "run",
            TEST_FILE,
            "--maxWorkers=1",
            "--no-file-parallelism",
            "-t",
            pattern(cases),
            "--reporter=default",
            "--reporter=json",
            "--outputFile=" + str(results),
        ],
        cwd=ROOT,
        env=env,
        check=True,
    )
    validate_results(results, cases)
    print(f"Recovery partition {args.group}: {len(cases)} cases passed")


if __name__ == "__main__":
    try:
        main()
    except (
        ValueError,
        KeyError,
        TypeError,
        OSError,
        subprocess.CalledProcessError,
    ) as error:
        print(f"recovery-partition: {error}", file=sys.stderr)
        sys.exit(1)
