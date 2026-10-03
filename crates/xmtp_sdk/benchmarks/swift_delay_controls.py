#!/usr/bin/env python3
"""Compile the exact delay helper prefix with retained boundary controls."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parent
output = Path(sys.argv[1]).resolve()
output.mkdir(parents=True, exist_ok=True)
source = (root / "hosts/SwiftSupport.swift").read_text()
assert source.count("func now() -> Double {") == 1
helper = source.split("func now() -> Double {", 1)[0]
(output / "Delay.swift").write_text(helper)
control = root / "controls/DelayControl.swift"
binary = output / "delay-control"
subprocess.run(
    [
        "swiftc",
        "-parse-as-library",
        str(output / "Delay.swift"),
        str(control),
        "-o",
        str(binary),
    ],
    check=True,
)
subprocess.run([str(binary)], check=True)
print(
    json.dumps(
        {
            "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
            "helper_sha256": hashlib.sha256(helper.encode()).hexdigest(),
            "control_sha256": hashlib.sha256(control.read_bytes()).hexdigest(),
        }
    )
)
