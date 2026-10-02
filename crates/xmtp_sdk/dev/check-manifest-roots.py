#!/usr/bin/env python3
"""Check that the SDK API manifest names real TypeScript root exports.

Each Node or browser `binding re-export` row that keeps a name, as generated
or static runtime, claims that the facade package root exports that name. The
Node root is `typescript-napi/index.ts`; the browser package exports its worker
root and its pure module root. A rename names the new export; a removal has no
final name and is not checked.
"""

from __future__ import annotations

from pathlib import Path
import re
import os
import sys

ROOT = Path(__file__).resolve().parents[3]
MANIFEST = ROOT / "docs/self-hosted/sdk-api-manifest.md"
sys.path.insert(0, str(ROOT / "dev/sdk"))
from switches import switched_sdks

GENERATED = Path(os.environ.get("XMTP_SDK_GENERATED_DIR", ROOT / "target/sdk-generated"))
TREES = {
    "Node": ["typescript-napi"],
    "Browser": ["typescript-wasm", "typescript-pure"],
}
CHECKED_KINDS = {"binding re-export"}


def root_exports(tree: str) -> set[str]:
    index = GENERATED / tree / "index.ts"
    if not index.exists():
        raise SystemExit(f"missing {index}; run 'just sdk generate' first")
    names: set[str] = set()
    for group in re.findall(r"export (?:type )?\{([^}]*)\}", index.read_text()):
        for part in group.split(","):
            part = re.sub(r"^type\s+", "", part.strip())
            if part:
                names.add(part.split(" as ")[-1].strip())
    return names


def main() -> None:
    switched = switched_sdks(ROOT)
    exports = {
        sdk: set().union(*(root_exports(tree) for tree in trees))
        for sdk, trees in TREES.items()
        if not switched or sdk in switched
    }
    section = None
    errors = []
    checked = 0
    for line in MANIFEST.read_text().splitlines():
        heading = re.match(r"^## (\w+)", line)
        if heading:
            section = heading.group(1)
            continue
        if section not in exports or not line.startswith("| `"):
            continue
        cells = [cell.strip() for cell in line.split("|")[1:-1]]
        current, kind, final, status = cells[0], cells[1], cells[2], cells[3]
        if kind not in CHECKED_KINDS or status in {
            "approved removal",
            "proposed removal",
        }:
            continue
        name = re.sub(r"^func ", "", final.strip("`"))
        if "." in name or "(" in name:
            continue
        checked += 1
        if name not in exports[section]:
            errors.append(
                f"{section} {current}: final name {name} is not a root export"
            )
    if errors:
        print("\n".join(errors), file=sys.stderr)
        raise SystemExit(1)
    print(f"{checked} manifest re-export rows name real Node and browser root exports")


if __name__ == "__main__":
    main()
