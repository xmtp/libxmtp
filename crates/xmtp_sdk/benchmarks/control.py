#!/usr/bin/env python3
"""Exercise all four runner paths with explicit synthetic process controls."""

import json
import sys
from pathlib import Path

from fixtures import dataset, digest, expected_observation
from packages import ROLES
from runner import run
from test_gate import configuration


def host():
    request = json.load(sys.stdin)
    response = {"request_sha256": digest(request), "ready": True}
    if request["phase"] == "measure":
        pair = request["pair"]
        response.update(
            duration_ms=100 + pair,
            peak_memory_bytes=1000000 + pair,
            observation=expected_observation(dataset(), request["workload"]),
            safety={
                "correctness": True,
                "deadlock": False,
                "use_after_end": None,
                "retained_growth": None,
            },
            long_tasks_ms=[51] if pair % 2 else [],
            mobile_lift={
                "record_ms": 100,
                "class_ms": 100,
                "order": ["record", "class"] if pair % 2 == 0 else ["class", "record"],
            },
        )
    print(json.dumps(response))


def main():
    if sys.argv[1] == "--host":
        host()
        return
    output = Path(sys.argv[1]).resolve()
    output.mkdir(parents=True, exist_ok=False)
    for target in ROLES:
        config = configuration(target)
        for side in ("old", "new"):
            root = output / (target + "-" + side)
            root.mkdir()
            (root / "control.asset").write_text(
                "Synthetic package bytes. No SDK code.\n"
            )
            config[side].update(
                root=str(root),
                assets={role: ["control.asset"] for role in ROLES[target]},
                command=[sys.executable, str(Path(__file__).resolve()), "--host"],
            )
        config_path = output / (target + "-config.json")
        config_path.write_text(json.dumps(config))
        if run(config_path, output / target, target) != 0:
            raise ValueError(f"Synthetic equal control failed for {target}")
        print(
            f"PASS {target}: 20 paired subprocess runs per workload; release gate PENDING",
            flush=True,
        )


if __name__ == "__main__":
    main()
