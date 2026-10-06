#!/usr/bin/env python3
"""Check required bytes in the actual SDK compiler source outputs."""

import importlib.util
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EXPRESSION = r"""
systems:
let
  p = systems.${builtins.currentSystem};
  names = [ "xmtp-sdk-libs" "xmtp-sdk-bindgen" "xmtp-sdk-wasm"
    "xmtp-sdk-pure-wasm" "xmtp-sdk-android-arm64-v8a" ]
    ++ (if builtins.hasAttr "xmtp-sdk-ios-device" p
        then [ "xmtp-sdk-ios-device" "xmtp-sdk-ios-simulator" ] else []);
in builtins.listToAttrs (map (name: {
  inherit name;
  value = let compiler = p.${name}.compilation or p.${name};
    in { path = toString compiler.src; drv = compiler.src.drvPath; };
}) names)
"""


def main():
    spec = importlib.util.spec_from_file_location(
        "artifacts", ROOT / "crates/xmtp_sdk/dev/sdk-artifacts.py"
    )
    artifacts = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(artifacts)
    products = json.loads(
        subprocess.check_output(
            ["nix", "eval", "--impure", "--json", ".#packages", "--apply", EXPRESSION],
            cwd=ROOT,
        )
    )
    subprocess.run(
        [
            "nix-store",
            "--realise",
            *sorted({item["drv"] for item in products.values()}),
        ],
        cwd=ROOT,
        check=True,
    )
    sdk = [
        "crates/xmtp_sdk/src/lib.rs",
        "crates/xmtp_proto/build.rs",
        *artifacts.COMPILE_INPUTS,
    ]
    sdk.extend(
        str(path.relative_to(ROOT)) for path in (ROOT / "proto").rglob("*.proto")
    )
    sdk.extend(
        str(path.relative_to(ROOT))
        for path in (ROOT / "crates/xmtp_db/migrations").rglob("*.sql")
    )
    bindgen = [
        "apps/xmtp_sdk_bindgen/src/main.rs",
        "apps/xmtp_sdk_bindgen/src/swift_event_fixture.swift",
        "apps/xmtp_sdk_bindgen/runtime/ts/bridge/worker/host.ts",
        "crates/xmtp_sdk/src/client/event_conformance.rs",
        "crates/xmtp_sdk/src/foreign_conformance.rs",
    ]
    bindgen.extend(
        str(path.relative_to(ROOT))
        for path in (ROOT / "apps/xmtp_sdk_bindgen/templates").rglob("*")
        if path.is_file()
    )
    for name, item in products.items():
        source = Path(item["path"])
        required = bindgen if name == "xmtp-sdk-bindgen" else sdk
        for relative in required:
            actual = source / relative
            if (
                not actual.is_file()
                or actual.read_bytes() != (ROOT / relative).read_bytes()
            ):
                raise ValueError(f"{name}: compiler source omits or changes {relative}")
        if (source / "Cargo.lock").read_bytes() != (ROOT / "Cargo.lock").read_bytes():
            raise ValueError(f"{name}: compiler source changes Cargo.lock")
        print(f"{name}: checked {len(required)} required compiler files and Cargo.lock")


if __name__ == "__main__":
    main()
