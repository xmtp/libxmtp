#!/usr/bin/env python3
"""Admit only the already validated, frozen RC5 release to recovery."""

import json
import sys
from pathlib import Path

SOURCE = "690af7ce11df9419e5dafb62d309d71c48c6cc99"
RUN = 43899974572


def verify(run, pages, plan):
    expected_run = {
        "id": RUN,
        "head_sha": SOURCE,
        "head_branch": "release/8.0.0",
        "event": "workflow_dispatch",
        "path": ".github/workflows/release.yml",
        "repository": {"full_name": "xmtp/libxmtp"},
    }
    for key, expected in expected_run.items():
        actual = run.get(key)
        if key == "repository":
            actual = {"full_name": actual.get("full_name")} if actual else None
        if actual != expected:
            raise ValueError(f"Original RC5 run has unexpected {key}")
    for key, expected in {
        "sourceSha": SOURCE,
        "version": "8.0.0-rc5",
        "targetVersion": "8.0.0",
        "kind": "rc",
        "rcNumber": 5,
    }.items():
        if plan.get(key) != expected:
            raise ValueError(f"Frozen RC5 plan has unexpected {key}")
    gates = [
        job
        for page in pages
        for job in page["jobs"]
        if job["name"] == "All six SDKs and selected CLI validated"
    ]
    if len(gates) != 1 or gates[0].get("conclusion") != "success":
        raise ValueError("Original all-six SDK and CLI validation must have succeeded")


if __name__ == "__main__":
    verify(*(json.loads(Path(filename).read_text()) for filename in sys.argv[1:]))
