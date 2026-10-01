#!/usr/bin/env python3
"""Run live stream regressions on Node, Chromium, Swift, and Kotlin."""

import argparse
import hashlib
import json
import os
import shlex
import subprocess
from pathlib import Path

from fixtures import canonical, dataset


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output")
    parser.add_argument("--browser-tools", required=True)
    parser.add_argument("--before-commit")
    args = parser.parse_args()
    source = Path(__file__).resolve().parent
    root = source.parents[2]
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixture = output / "fixture.json"
    fixture.write_bytes(canonical(dataset()))
    outcomes = []

    def run(label, command, shell="default", expected_failure=None):
        result = subprocess.run(
            [
                str(root / "dev/nix-shell"),
                shlex.join([str(value) for value in command]),
            ],
            cwd=root,
            env=dict(os.environ, NIX_DEVSHELL=shell),
            capture_output=True,
            text=True,
        )
        (output / (label + ".stdout")).write_text(result.stdout)
        (output / (label + ".stderr")).write_text(result.stderr)
        valid = (
            result.returncode != 0 and expected_failure in result.stdout + result.stderr
            if expected_failure
            else result.returncode == 0
        )
        outcomes.append(
            {
                "label": label,
                "argv": [str(value) for value in command],
                "exit_code": result.returncode,
                "expected_failure": expected_failure,
                "passed": valid,
            }
        )
        (output / "commands.json").write_text(json.dumps(outcomes, indent=2) + "\n")
        if not valid:
            raise RuntimeError(
                f"Unexpected control result: {label}; see saved stdout/stderr"
            )
        print(f"PASS {label}", flush=True)

    weaker = output / "weaker-workload.mjs"
    if args.before_commit:
        weaker.write_bytes(
            subprocess.check_output(
                [
                    "git",
                    "show",
                    f"{args.before_commit}:crates/xmtp_sdk/benchmarks/hosts/workload.mjs",
                ],
                cwd=root,
            )
        )
    else:
        text = (source / "hosts/workload.mjs").read_text()
        before = "live.push(api.live(message));"
        assert before in text
        text = text.replace(before, "// Weaker control: discard live content.")
        before = "const messages = enrichLive(live, state.ids);"
        assert before in text
        text = text.replace(
            before,
            "const messages = (await api.page(receivedGroup, 10000)).map((message) => api.normalize(message, keyById));",
        )
        # The weakened helper does not need the live module in this temp directory.
        text = text.replace('import { enrichLive } from "./live.mjs";', "")
        weaker.write_text(text)
    for target in ("node", "browser"):
        command = ["node", source / f"stream_control_{target}.mjs", fixture]
        if target == "browser":
            command.append(Path(args.browser_tools).resolve())
        run(target + "-red", command + [weaker], expected_failure='"rejected":false')
        run(target + "-restored", command)

    weak_swift = output / "SwiftLive.swift"
    text = (source / "hosts/SwiftLive.swift").read_text()
    weak_swift.write_text(
        text[: text.index("func enrichLive(")]
        + "func enrichLive(_ events: [LiveEvent], _ ids: [String]) throws -> [FixtureMessage] { controlHistory }\n"
    )
    swift_control = source / "controls/StreamControl.swift"
    for name, live in (
        ("red", weak_swift),
        ("restored", source / "hosts/SwiftLive.swift"),
    ):
        executable = output / ("swift-" + name)
        run(
            "swift-" + name + "-compile",
            [
                "swiftc",
                "-O",
                "-swift-version",
                "5",
                live,
                swift_control,
                "-o",
                executable,
            ],
            "ios",
        )
        run(
            "swift-" + name,
            [executable, fixture],
            "ios",
            "Live control did not detect the fault" if name == "red" else None,
        )

    weak_kotlin = output / "Live.kt"
    text = (source / "hosts/android/Live.kt").read_text()
    weak_kotlin.write_text(
        text[: text.index("fun enrichLive(")]
        + "fun enrichLive(events: List<LiveEvent>, ids: List<String>): List<LiveRow> = controlHistory\n"
    )
    kotlin_control = source / "controls/StreamControl.kt"
    for name, live in (
        ("red", weak_kotlin),
        ("restored", source / "hosts/android/Live.kt"),
    ):
        archive = output / ("kotlin-" + name + ".jar")
        run(
            "kotlin-" + name + "-compile",
            [
                "kotlinc",
                "-jvm-target",
                "17",
                live,
                kotlin_control,
                "-include-runtime",
                "-d",
                archive,
            ],
            "android",
        )
        run(
            "kotlin-" + name,
            ["java", "-jar", archive],
            "android",
            "Live control did not detect the fault" if name == "red" else None,
        )
    receipt = {
        "purpose": "harness-control",
        "release_gate": "PENDING",
        "before_commit": args.before_commit,
        "commands": outcomes,
        "source_sha256": {
            str(path.relative_to(source)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in source.rglob("*")
            if path.is_file() and path.suffix in {".mjs", ".swift", ".kt", ".py"}
        },
    }
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
