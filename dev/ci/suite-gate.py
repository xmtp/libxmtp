#!/usr/bin/env python3
"""Require each selected suite and its fixed workflow result."""

import argparse
import importlib.util
import json
from pathlib import Path
import sys

INVENTORY = {
    "source": ("lint_workspace", "lint_js", "lint_config", "lint_proto"),
    "tests": (
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
    ),
}
spec = importlib.util.spec_from_file_location(
    "gate", Path(__file__).with_name("check-gate.py")
)
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


def check(kind, selection, needs, suites="", row=""):
    if (
        not isinstance(selection, dict)
        or type(selection.get("schema_version")) is not int
        or selection["schema_version"] != 1
        or not isinstance(selection.get("checks"), dict)
    ):
        raise ValueError("Invalid suite selection")
    inventory = INVENTORY[kind]
    checks = selection["checks"]
    if any(type(checks.get(key)) is not bool for key in inventory):
        raise ValueError("Suite selections must be complete booleans")
    if kind == "tests" and any(
        key in checks for key in ("check_bindings_ios", "check_bindings_android")
    ):
        flags = [
            checks.get(key) for key in ("check_bindings_ios", "check_bindings_android")
        ]
        if any(type(value) is not bool for value in flags) or checks[
            "test_bindings"
        ] != any(flags):
            raise ValueError("Binding suite differs from selected platforms")
    expected = [key for key in inventory if checks[key]]
    aggregate = "source_lint" if kind == "source" else "tests"
    if aggregate in checks and (
        type(checks[aggregate]) is not bool or checks[aggregate] != bool(expected)
    ):
        raise ValueError("Suite aggregate differs from selected checks")
    actual = json.loads(suites) if suites else expected
    if not isinstance(actual, list) or actual != expected:
        raise ValueError("Suite rows differ from selected checks")
    if row:
        if row not in expected:
            raise ValueError("Unknown or unselected suite row")
        mapping = {row: row}
    else:
        mapping = {key: "suites" for key in inventory}
    gate.check_gate(selection, needs, mapping, detector="")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", choices=INVENTORY, required=True)
    parser.add_argument("--selection", required=True)
    parser.add_argument("--needs", required=True)
    parser.add_argument("--suites", default="")
    parser.add_argument("--row", default="")
    args = parser.parse_args()
    check(
        args.kind,
        json.loads(args.selection),
        json.loads(args.needs),
        args.suites,
        args.row,
    )
    print("All selected suite results succeeded")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f"Suite gate failed: {error}", file=sys.stderr)
        sys.exit(1)
