#!/usr/bin/env python3
"""Require success for each selected producer and check."""

import argparse
import json
import sys


def check_gate(selection, needs, mapping, detector="detect-changes"):
    if selection.get("schema_version") != 1 or not isinstance(
        selection.get("checks"), dict
    ):
        raise ValueError("Missing or invalid CI selection")
    if detector and needs.get(detector, {}).get("result") != "success":
        raise ValueError("Change detection did not succeed")
    checks = selection["checks"]
    for key, job in mapping.items():
        if type(checks.get(key)) is not bool:
            raise ValueError(f"Missing boolean selection: {key}")
        result = needs.get(job, {}).get("result")
        if result not in ("success", "skipped", "failure", "cancelled"):
            raise ValueError(f"Missing or invalid result: {job}")
        if result in ("failure", "cancelled") or (checks[key] and result != "success"):
            raise ValueError(f"Required CI job did not succeed: {job} ({result})")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selection", required=True)
    parser.add_argument("--needs", required=True)
    parser.add_argument("--mapping", required=True)
    parser.add_argument("--detector", default="detect-changes")
    args = parser.parse_args()
    check_gate(
        json.loads(args.selection),
        json.loads(args.needs),
        json.loads(args.mapping),
        args.detector,
    )
    print("All selected CI jobs succeeded")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError) as error:
        print(f"CI gate failed: {error}", file=sys.stderr)
        sys.exit(1)
