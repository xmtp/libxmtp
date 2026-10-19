"""Record initial local RN roots and bounded build markers without realizing paths.

The signed component/package exports and consumer build receipts own output NARs.
"""

import json
import os
from pathlib import Path
import subprocess
import sys


def output(*command):
    return subprocess.check_output(command, text=True).strip()


def eval_json(expression):
    args = ["nix", "eval", "--json", "--impure", "--expr", expression]
    return json.loads(
        output(*args, "--option", "allow-import-from-derivation", "false")
    )


stage, system, version, mode, phase = sys.argv[1:]
path = Path(f"target/rn-evidence/{stage}-{phase}.json")
if phase == "before":
    # Versioned bundles and their members must use the release package too.
    expression = """
      let f = builtins.getFlake ("git+file://" + toString ./.);
          system = builtins.getEnv "RN_SYSTEM";
          stage = builtins.getEnv "RN_STAGE";
          members = f.ciStages.${system}.${stage}.passthru.members;
          version = builtins.getEnv "RN_PACKAGE_VERSION";
          consumers = f.legacyPackages.${system}.reactNative.mkConsumers {
            package = f.legacyPackages.x86_64-linux.reactNative.mkPackage { inherit version; };
          };
          roots = if stage == "components" then members // f.ciStages.${system}.native-base.passthru.members
                  else if version == "" then members
                  else consumers.passthru.members // { bundles = consumers; }
                    // builtins.intersectAttrs { react-native-consumer-template-bare = null; react-native-consumer-template-expo = null; } members;
      in builtins.mapAttrs (_: drv: { derivationPath = drv.drvPath; outputPath = drv.outPath; }) roots
    """
    roots = eval_json(expression)
    receipt = {
        "sourceSha": output("git", "rev-parse", "HEAD"),
        "system": system,
        "stage": stage,
        "version": version,
        "cacheMode": mode,
        "runId": os.environ.get("GITHUB_RUN_ID"),
        "roots": roots,
    }
    if system == "aarch64-darwin":
        receipt["xcode"] = eval_json("""
          let f = builtins.getFlake ("git+file://" + toString ./.);
          in { native = f.lib.xcode; rn = f.lib.reactNativeXcode; }
        """)
        receipt["xcode"]["processDeveloperDir"] = os.environ.get("DEVELOPER_DIR")
        receipt["xcode"]["globalDeveloperDir"] = output(
            "/usr/bin/env", "-u", "DEVELOPER_DIR", "/usr/bin/xcode-select", "-p"
        )
        receipt["xcode"]["version"] = output(
            "/usr/bin/env",
            "DEVELOPER_DIR=" + receipt["xcode"]["globalDeveloperDir"],
            "/usr/bin/xcodebuild",
            "-version",
        )
    paths = [root["outputPath"] for root in roots.values()]
    command = ["nix-store", "--check-validity", "--print-invalid"]
    invalid = set(output(*command, *paths).splitlines())
    for root in roots.values():
        root["validBefore"] = root["outputPath"] not in invalid

else:
    receipt = json.loads(path.with_name(f"{stage}-before.json").read_text())
    elapsed = os.environ.get("RN_BUILD_SECONDS")
    receipt["buildSeconds"] = int(elapsed) if elapsed else None
    log = Path(os.environ["RUNNER_TEMP"], "rn-build.log")
    markers = []
    if log.exists():
        for line in log.read_text().splitlines():
            if "building '" in line or "copying path '" in line:
                markers.append(line)
    receipt["realizationMarkers"] = markers[-200:]

path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(receipt | {"phase": phase}, indent=2, sort_keys=True) + "\n")
