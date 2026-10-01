#!/usr/bin/env python3
"""Record the generator, bindings, and copied assets in a generated tree."""

import argparse
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("generated", type=Path)
parser.add_argument("--artifact", action="append", default=[])
args = parser.parse_args()


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


generator = {
    str(path.relative_to(root)): checksum(path)
    for path in sorted((root / "apps/xmtp_sdk_bindgen").rglob("*"))
    if path.is_file()
}
binaries = {
    Path(path).name + ":" + str(i): checksum(Path(path))
    for i, path in enumerate(args.artifact)
}
contract = hashlib.sha256(
    json.dumps([generator, binaries], sort_keys=True).encode()
).hexdigest()
for tree in args.generated.iterdir():
    if not tree.is_dir() or tree.name == "runtimes":
        continue
    files = {
        str(path.relative_to(tree)): checksum(path)
        for path in sorted(tree.rglob("*"))
        if path.is_file()
        and path.name != "sdk-contract.json"
        and "node_modules" not in path.parts
    }
    (tree / "sdk-contract.json").write_text(
        json.dumps(
            {
                "contract": contract,
                "generator": hashlib.sha256(
                    json.dumps(generator, sort_keys=True).encode()
                ).hexdigest(),
                "artifacts": binaries,
                "files": files,
            },
            indent=2,
        )
        + "\n"
    )
