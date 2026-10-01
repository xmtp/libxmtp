#!/usr/bin/env python3
"""Require useful old or weaker caller failures, then rerun restored controls."""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    args = parser.parse_args()
    source = Path(__file__).resolve().parent
    root = source.parents[2]
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    records = []
    with tempfile.TemporaryDirectory(prefix="benchmark-repair-controls-") as temporary:
        copy = Path(temporary) / "benchmarks"
        shutil.copytree(source, copy, ignore=shutil.ignore_patterns("__pycache__"))

        def run(label, command, failing=False, reason="AssertionError"):
            result = subprocess.run(command, cwd=copy, text=True, capture_output=True)
            (output / f"{label}.stdout").write_text(result.stdout)
            (output / f"{label}.stderr").write_text(result.stderr)
            valid = (
                result.returncode != 0 and reason in result.stdout + result.stderr
                if failing
                else result.returncode == 0
            )
            records.append(
                {
                    "label": label,
                    "argv": command,
                    "exit_code": result.returncode,
                    "expected_failure": failing,
                    "passed": valid,
                }
            )
            (output / "commands.json").write_text(json.dumps(records, indent=2) + "\n")
            if not valid:
                raise RuntimeError(f"Unexpected control result: {label}")

        def python_case(name):
            return [
                sys.executable,
                "-B",
                "-m",
                "unittest",
                "test_repairs." + name,
                "-v",
            ]

        def mutate(label, path, before, after, command, reason="AssertionError"):
            file = copy / path
            original = file.read_text()
            if before not in original:
                raise ValueError(f"Missing mutation anchor: {label}")
            file.write_text(original.replace(before, after))
            try:
                run(label + "-weak", command, failing=True, reason=reason)
            finally:
                file.write_text(original)
            run(label + "-restored", command)

        original_driver = subprocess.check_output(
            [
                "git",
                "show",
                "c72798ac9088a4500dfcfda4b5907fb572d2f1d5:crates/xmtp_sdk/benchmarks/hosts/driver.py",
            ],
            cwd=root,
        ).decode()
        current = (copy / "hosts/driver.py").read_text()
        for name, case in [
            ("safety", "DriverControls.test_actual_driver_safety_evidence"),
            ("page-order", "DriverControls.test_observed_page_order"),
        ]:
            (copy / "hosts/driver.py").write_text(original_driver)
            run(name + "-old", python_case(case), failing=True)
            (copy / "hosts/driver.py").write_text(current)
            run(name + "-restored", python_case(case))
        mutate(
            "archive-bytes",
            "baselines.py",
            "if actual != checksum:",
            "if False:",
            python_case("ResolverControls.test_archive_bytes_before_lock"),
        )
        mutate(
            "android-source",
            "baselines.py",
            'if re.findall(r"^version=(.+)$", source.decode(), re.MULTILINE) != [version]:',
            "if False:",
            python_case("ResolverControls.test_missing_or_conflicting_android_source"),
        )
        original_sampler = subprocess.check_output(
            [
                "git",
                "show",
                "c72798ac9088a4500dfcfda4b5907fb572d2f1d5:crates/xmtp_sdk/benchmarks/hosts/processes.py",
            ],
            cwd=root,
        ).decode()
        current = (copy / "hosts/processes.py").read_text()
        (copy / "hosts/processes.py").write_text(original_sampler)
        # The old sampler raises RuntimeError instead of reaching the assertion.
        command = python_case("SamplerControls.test_delayed_thread_start_short_child")
        result = subprocess.run(command, cwd=copy, text=True, capture_output=True)
        (output / "sampler-old.stdout").write_text(result.stdout)
        (output / "sampler-old.stderr").write_text(result.stderr)
        valid = (
            result.returncode != 0
            and "Process-tree memory could not be sampled" in result.stderr
        )
        records.append(
            {
                "label": "sampler-old",
                "argv": command,
                "exit_code": result.returncode,
                "expected_failure": True,
                "passed": valid,
            }
        )
        if not valid:
            raise RuntimeError("Old sampler did not lose the short child sample")
        (copy / "hosts/processes.py").write_text(current)
        run("sampler-restored", command)
        for target in ("node", "browser"):
            mutate(
                target + "-public-entry",
                f"hosts/{target}.mjs",
                f'await admitEntries(config, request, "{target}");',
                "",
                ["node", "entry_controls.mjs"],
            )
        mutate(
            "reaction-count",
            "hosts/live.mjs",
            "(delivered.get(key) ?? 0) < count",
            "!delivered.has(key)",
            ["node", "reaction_controls.mjs"],
        )
        fixture = output / "fixture.json"
        from fixtures import canonical, dataset

        fixture.write_bytes(canonical(dataset()))
        mutate(
            "stream-duplicate",
            "hosts/workload.mjs",
            'if (seen.has(message.id))\n            throw new Error("Duplicate expected stream event");',
            "if (seen.has(message.id)) continue;",
            ["node", "stream_control_node.mjs", str(fixture)],
            reason='"fault":"duplicate_expected","rejected":false',
        )
        run(
            "restored-python",
            [sys.executable, "-B", "-m", "unittest", "test_repairs", "-v"],
        )
    print(json.dumps(records, indent=2))


if __name__ == "__main__":
    main()
