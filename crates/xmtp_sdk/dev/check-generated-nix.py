#!/usr/bin/env python3
"""Check selected SDK generation and Android consumer build closures."""

import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EXPRESSION = r"""
systems:
let
  p = systems.${builtins.currentSystem};
  names = [ "xmtp-sdk-generated" "xmtp-sdk-generated-swift"
    "xmtp-sdk-generated-kotlin" "xmtp-sdk-generated-node"
    "xmtp-sdk-generated-browser" "xmtp-sdk-libs" "xmtp-sdk-bindgen"
    "xmtp-sdk-wasm" "xmtp-sdk-pure-wasm"
    "android-sdk-libs-fast" "android-sdk-libs" ]
    ++ (if builtins.hasAttr "ios-libs" p then [ "ios-libs" "ios-libs-fast" ] else []);
in builtins.listToAttrs (map (name: {
  inherit name;
  value = p.${name}.drvPath;
}) names)
"""


def main():
    products = json.loads(
        subprocess.check_output(
            ["nix", "eval", "--impure", "--json", ".#packages", "--apply", EXPRESSION],
            cwd=ROOT,
        )
    )
    print(json.dumps(products, indent=2, sort_keys=True), flush=True)
    closures = {}
    for name, path in products.items():
        if name in (
            "xmtp-sdk-libs",
            "xmtp-sdk-bindgen",
            "xmtp-sdk-wasm",
            "xmtp-sdk-pure-wasm",
        ):
            continue
        closures[name] = set(
            subprocess.check_output(
                ["nix-store", "--query", "--requisites", path],
                cwd=ROOT,
                text=True,
            ).splitlines()
        )

    failures = []

    def require(name, required, forbidden):
        closure = closures[name]
        for dependency in required:
            if products[dependency] not in closure:
                failures.append(f"{name}: missing {dependency} in build closure")
        for dependency in forbidden:
            if products[dependency] in closure:
                failures.append(f"{name}: unexpected {dependency} in build closure")
        print(f"{name}: checked {len(closure)} build inputs")

    wasm = ("xmtp-sdk-wasm", "xmtp-sdk-pure-wasm")
    for language in ("swift", "kotlin", "node"):
        require(
            f"xmtp-sdk-generated-{language}",
            ("xmtp-sdk-libs", "xmtp-sdk-bindgen"),
            wasm,
        )
    require(
        "xmtp-sdk-generated-browser", (*wasm, "xmtp-sdk-bindgen"), ("xmtp-sdk-libs",)
    )
    require("xmtp-sdk-generated", (*wasm, "xmtp-sdk-libs", "xmtp-sdk-bindgen"), ())
    for name in ("android-sdk-libs-fast", "android-sdk-libs"):
        require(name, ("xmtp-sdk-generated-kotlin",), (*wasm, "xmtp-sdk-generated"))
    for name in ("ios-libs", "ios-libs-fast"):
        if name in closures:
            require(name, ("xmtp-sdk-generated-swift",), (*wasm, "xmtp-sdk-generated"))
    if failures:
        raise ValueError("\n".join(failures))


if __name__ == "__main__":
    main()
