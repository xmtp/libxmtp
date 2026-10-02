#!/usr/bin/env python3
"""Check source identity through the real generated and native Nix filters."""

import importlib.util
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EXPRESSION = r"""
systems:
let
  p = systems.${builtins.currentSystem};
  names = [ "xmtp-sdk-generated" "xmtp-sdk-libs" ]
    ++ (if builtins.hasAttr "xmtp-sdk-android-arm64-v8a" p
        then [ "xmtp-sdk-android-arm64-v8a" ] else []);
in builtins.listToAttrs (map (name: {
  inherit name;
  value = toString p.${name}.src;
}) names)
"""


def main():
    spec = importlib.util.spec_from_file_location(
        "sdk_artifacts", ROOT / "crates/xmtp_sdk/dev/sdk-artifacts.py"
    )
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    expected = {
        "native": artifacts.source_hash(),
        "generator": artifacts.source_hash(True),
    }
    sources = json.loads(
        subprocess.check_output(
            ["nix", "eval", "--impure", "--json", ".#packages", "--apply", EXPRESSION],
            cwd=ROOT,
        )
    )
    results = {}
    for name, source in sources.items():
        artifacts.ROOT = Path(source)
        actual = {
            "native": artifacts.source_hash(),
            "generator": artifacts.source_hash(True),
        }
        results[name] = {"source": source, **actual}
    print(json.dumps({"checkout": expected, "products": results}, indent=2))
    for name, actual in results.items():
        for kind, digest in expected.items():
            if actual[kind] != digest:
                raise ValueError(
                    f"{name}: {kind} source identity differs from checkout"
                )


if __name__ == "__main__":
    main()
