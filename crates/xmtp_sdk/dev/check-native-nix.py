#!/usr/bin/env python3
"""Check the evaluated SDK native and dependency build inputs."""

import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EXPRESSION = r'''
systems:
let
  p = systems.${builtins.currentSystem};
  names = [ "xmtp-sdk-libs" "xmtp-sdk-bindgen" "xmtp-sdk-wasm" "xmtp-sdk-pure-wasm" ]
    ++ (if builtins.hasAttr "xmtp-sdk-ios-device" p
        then [ "xmtp-sdk-ios-device" "xmtp-sdk-ios-simulator" ] else []);
  inputs = d: {
    jobs = d.CARGO_BUILD_JOBS or null;
    vendor = d.OPENSSL_NO_VENDOR or null;
    static = d.OPENSSL_STATIC or null;
    macos = d.MACOSX_DEPLOYMENT_TARGET or null;
    command = d.buildPhaseCargoCommand or (d.buildPhase or "");
  };
in builtins.listToAttrs (map (name: {
  inherit name;
  value = { main = inputs p.${name}; deps = inputs p.${name}.cargoArtifacts; };
}) names)
'''

products = json.loads(subprocess.check_output(
    ["nix", "eval", "--impure", "--json", ".#packages", "--apply", EXPRESSION],
    cwd=ROOT,
))
for name, phases in products.items():
    native = name == "xmtp-sdk-libs" or name.startswith("xmtp-sdk-ios-")
    for phase, inputs in phases.items():
        if inputs["jobs"] != 2:
            raise ValueError((name, phase, "Cargo job limit", inputs))
        if native:
            if inputs["vendor"] != "0":
                raise ValueError((name, phase, "vendored OpenSSL", inputs))
            if inputs["static"] != "1":
                raise ValueError((name, phase, "static OpenSSL", inputs))
            if "xmtp-sdk-ios-device" in products:
                if inputs["macos"] != "11.0":
                    raise ValueError((name, phase, "macOS floor", inputs))
        if name.startswith("xmtp-sdk-ios-"):
            if 'export IPHONEOS_DEPLOYMENT_TARGET="14"' not in inputs["command"]:
                raise ValueError((name, phase, "iOS floor", inputs))
print(json.dumps(products, indent=2, sort_keys=True))
